import "./App.css";
import { Route, Switch } from "wouter";
import NotePage from "./NotePage.tsx";
import { useReducer, useEffect } from "react";
import { initializeWebSocket } from "./socket.ts";
import { Graph, updateNodeEdges } from "./graph.ts";

type State = {
  graph: Graph;
  // TODO: Change to more stuff if/when needed
  titles: Map<string, string>;
};

type Action = {
  type: "update";
  payload: {
    id: string;
    title: string;
    links: string[];
    backlinks: string[];
  };
};

function reducer(state: State, action: Action): State {
  switch (action.type) {
    case "update": {
      const { id, title, links, backlinks } = action.payload;
      const newGraph = updateNodeEdges(
        state.graph,
        id,
        new Set(links),
        new Set(backlinks),
      );
      const newTitles = new Map(state.titles);
      newTitles.set(id, title);
      return {
        graph: newGraph,
        titles: newTitles,
      };
    }
    default:
      return state;
  }
}

const initialState: State = {
  graph: { outgoing: new Map(), incoming: new Map() },
  titles: new Map(),
};

export default function App() {
  const [state, dispatch] = useReducer(reducer, initialState);

  useEffect(() => {
    const socket = initializeWebSocket();
    return () => {
      socket.close();
    };
  }, []);

  return (
    <Switch>
      <Route path="/note/:id">
        {({ id }) => {
          // TODO: Show a proper loading page not this nonsense
          const title = state.titles.get(id) ?? "Loading...";
          const links = state.graph.outgoing.get(id) ?? new Set();
          const backlinks = state.graph.incoming.get(id) ?? new Set();
          return (
            <NotePage
              id={id}
              title={title}
              links={links}
              backlinks={backlinks}
            />
          );
        }}
      </Route>
    </Switch>
  );
}
