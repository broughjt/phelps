import { Graph } from "./graph";

type Content = {
  html: string;
  status: "loaded" | "dirty" | "loading";
};

type State = {
  graph: Graph;
  // TODO: Change to more stuff if/when needed
  titles: Record<string, string>;
  content: Record<string, Content>;
  initialized: boolean;
};

export type Update = {
  id: string;
  title: string;
  links: string[];
};

export type Initialize = {
  outgoing_links: Record<string, string[]>;
  titles: Record<string, string>;
};

export type Action =
  | {
      type: "building";
    }
  | {
      type: "initialize";
      payload: Initialize;
    }
  | {
      type: "update";
      payload: Update[];
    }
  | {
      type: "remove";
      payload: string[];
    };

export function reducer(state: State, action: Action): State {
  switch (action.type) {
    case "initialize": {
      const { outgoing_links, titles } = action.payload;

      console.log("Initialize");
      // TODO: If we add new state, we'll want to potentially keep existing
      // state around here in stead of throwing it away
      return {
        graph: Graph.fromOutgoing(outgoing_links),
        titles: titles,
        content: {},
        initialized: true,
      };
    }
    case "building": {
      // TODO: Do something interesting with building information
      return state;
    }
    case "update": {
      const updates = action.payload;

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
      const ids = action.payload;

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
  }
}

export const initialState: State = {
  graph: new Graph(),
  titles: {},
  content: {},
  initialized: false,
};
