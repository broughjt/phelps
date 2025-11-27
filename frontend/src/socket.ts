import { Action, Initialize, Update } from "./reducer";

export function handleSocketMessage(
  dispatch: (_: Action) => void,
  navigateToNote: (_: string) => void
) {
  return (event: MessageEvent) => {
    const message = JSON.parse(event.data);

    switch (message.tag) {
      case "building": {
        dispatch({ type: "building" });
        break;
      }
      case "initialize": {
        if (message.content) {
          const initialize: Initialize = {
            outgoingLinks: message.content.outgoing_links,
            titles: message.content.titles,
            defaultNote: message.content.default_note,
          };
          dispatch({ type: "initialize", initialize: initialize });
        } else {
          throw new Error("Missing content in initialize message");
        }
        break;
      }
      case "update": {
        if (message.content) {
          const updates: Update[] = message.content;
          dispatch({ type: "update", updates: updates });
        } else {
          throw new Error("Missing content in update message");
        }
        break;
      }
      case "remove": {
        if (message.content) {
          dispatch({ type: "remove", ids: message.content });
        } else {
          throw new Error("Missing content in remove message");
        }
        break;
      }
      case "focus": {
        if (message.content) {
          navigateToNote(message.content);
        } else {
          throw new Error("Missing content in focus message");
        }
        break;
      }
      default: {
        throw new Error(`Unknown message tag: ${message.tag}`);
      }
    }
  };
}
