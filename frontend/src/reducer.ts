import { Graph } from "./graph";

type Content = {
  html: string | null;
  status: "loaded" | "dirty" | "loading";
};

type State = {
  graph: Graph;
  // TODO: Change to more stuff if/when needed
  titles: Record<string, string>;
  content: Record<string, Content>;
  initialized: boolean;
  defaultNote: string | null;
};

export type Update = {
  id: string;
  title: string;
  links: string[];
};

export type Initialize = {
  outgoingLinks: Record<string, string[]>;
  titles: Record<string, string>;
  defaultNote: string;
};

export type Action =
  | {
      type: "building";
    }
  | {
      type: "initialize";
      initialize: Initialize;
    }
  | {
      type: "update";
      updates: Update[];
    }
  | {
      type: "remove";
      ids: string[];
    }
  | {
      type: "fetchingContent";
      id: string;
    }
  | {
      type: "setContent";
      id: string;
      html: string;
    };

export function reducer(state: State, action: Action): State {
  switch (action.type) {
    case "initialize": {
      const { outgoingLinks, titles, defaultNote } = action.initialize;

      // TODO: If we add new state, we'll want to potentially keep existing
      // state around here in stead of throwing it away
      return {
        graph: Graph.fromOutgoing(outgoingLinks),
        titles: titles,
        content: {},
        initialized: true,
        defaultNote: defaultNote,
      };
    }
    case "building": {
      // TODO: Do something interesting with building information
      return state;
    }
    case "update": {
      const updates = action.updates;

      const newTitles = { ...state.titles };
      const newContent = { ...state.content };
      const newGraph = state.graph.shallowCopy();

      for (const { id, title, links } of updates) {
        newTitles[id] = title;
        if (newContent[id]) {
          newContent[id].status = "dirty";
        }

        for (const j in links) {
          newGraph.addEdge(id, j);
        }
      }

      return {
        ...state,
        graph: newGraph,
        titles: newTitles,
        content: newContent,
      };
    }
    case "remove": {
      const ids = action.ids;

      const newTitles = { ...state.titles };
      const newContent = { ...state.content };
      const newGraph = state.graph.shallowCopy();

      for (const i of ids) {
        delete newTitles[i];
        delete newContent[i];
        newGraph.removeNode(i);
      }

      return state;
    }
    case "fetchingContent": {
      const id = action.id;

      const newContent = { ...state.content };
      if (!newContent[id]) {
        newContent[id] = { html: null, status: "loading" };
      } else {
        newContent[id].status = "loading";
      }

      return {
        ...state,
        content: newContent,
      };
    }
    case "setContent": {
      const id = action.id;
      const html = action.html;

      const newContent = { ...state.content };
      newContent[id] = { status: "loaded", html: html };

      return {
        ...state,
        content: newContent,
      };
    }
  }
}

export const initialState: State = {
  graph: new Graph(),
  titles: {},
  content: {},
  initialized: false,
  defaultNote: null,
};
