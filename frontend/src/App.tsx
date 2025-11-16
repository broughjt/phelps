import { useCallback, useEffect, useReducer } from "react";
import { initialState, reducer } from "./reducer";
import { handleSocketMessage } from "./socket";
import { Redirect, Route, Switch } from "wouter";
import { NotePage } from "./NotePage";
import { NotesApi } from "./api";

const HOST = "localhost:3000";
const API_URL = `http://${HOST}`;
const WEBSOCKET_URL = `ws://${HOST}/api/updates`;

const noteApi = new NotesApi(API_URL);

export default function App() {
  const [state, dispatch] = useReducer(reducer, initialState);
  const fetchNoteContent = useCallback(async (id: string) => {
    dispatch({ type: "fetchingContent", id });
    const html = await noteApi.getNoteContent(id);
    dispatch({ type: "setContent", id, html });
  }, []);

  useEffect(() => {
    const socket = new WebSocket(WEBSOCKET_URL);

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

  const notFound = <div>404, Not Found</div>;

  return (
    <Switch>
      <Route path="/note/:id">
        {({ id }) => {
          if (state.graph.hasNode(id)) {
            const title = state.titles[id];
            const backlinkIds = Array.from(
              state.graph.incomingEdges(id)!.values(),
            );
            const backlinks = Object.fromEntries(
              backlinkIds.map((id: string) => [id, state.titles[id]]),
            );
            const html = state.content[id]?.html;
            const status = state.content[id]?.status ?? "empty";

            return (
              <NotePage
                id={id}
                title={title}
                backlinks={backlinks}
                status={status}
                html={html}
                fetchNoteContent={fetchNoteContent}
              />
            );
          } else {
            return notFound;
          }
        }}
      </Route>
      <Route>
        {state.initialized ? (
          <Redirect to={`/note/${state.defaultNote}`} />
        ) : (
          <p>TODO: Loading</p>
        )}
      </Route>
    </Switch>
  );
}
