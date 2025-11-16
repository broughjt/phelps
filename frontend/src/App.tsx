import { useEffect, useReducer } from "react";
import { initialState, reducer } from "./reducer";
import { handleSocketMessage } from "./socket";
import { Route, Switch } from "wouter";

export default function App() {
  const [state, dispatch] = useReducer(reducer, initialState);

  useEffect(() => {
    const socket = new WebSocket("ws://localhost:3000/api/updates");

    socket.onclose = () => {
      console.log("WebSocket connection closed");
    };
    socket.onerror = (error) => {
      console.error("WebSocket error:", error);
    };

    socket.onmessage = handleSocketMessage(dispatch);

    return () => {
      socket.close();
    };
  }, []);

  const notFound = <div>404, Not Found!</div>;

  return (
    <Switch>
      <Route path="/note/:id">
        {({ id }) => {
          // // TODO: Show a proper loading page not this nonsense
          // // const title = state.titles.get(id) ?? "Loading...";
          // // const links = state.graph.outgoing.get(id) ?? new Set();
          // // const backlinks = state.graph.incoming.get(id) ?? new Set();
          // return (
          //   <NotePage
          //     id={id}
          //     title={title}
          //     links={links}
          //     backlinks={backlinks}
          //   />
          // );
          if (state.graph.hasNode(id)) {
            <p>Hello, world</p>;
          } else {
            return notFound;
          }
        }}
      </Route>
      <Route>{notFound}</Route>
    </Switch>
  );
}
