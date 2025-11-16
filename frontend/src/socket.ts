import { Action, Initialize, Update } from "./reducer";

type WebsocketMessage =
  | {
      tag: "building";
    }
  | {
      tag: "initialize";
      content: Initialize;
    }
  | {
      tag: "update";
      content: Update[];
    }
  | {
      tag: "remove";
      content: string[];
    };

export function handleSocketMessage(dispatch: (_: Action) => void) {
  return (event: MessageEvent) => {
    // TODO: Handle the possibility that returned JSON is not a valid
    // WebsocketMessage
    const message: WebsocketMessage = JSON.parse(event.data);
    console.log(message);

    switch (message.tag) {
      case "building":
        dispatch({ type: "building" });
        break;
      case "initialize":
        dispatch({ type: "initialize", payload: message.content });
        break;
      case "update":
        dispatch({ type: "update", payload: message.content });
        break;
      case "remove":
        dispatch({ type: "remove", payload: message.content });
        break;
    }
  };
}
