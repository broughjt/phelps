import { JSX, useEffect, useRef } from "react";
import { useLocation } from "wouter";

type NoteContentProperties = {
  id: string;
  status: "empty" | "loaded" | "dirty" | "loading";
  html: string | null;
  fetchNoteContent: (id: string) => Promise<void>;
};

export function NoteContent({
  id,
  status,
  html,
  fetchNoteContent,
}: NoteContentProperties): JSX.Element {
  const [, navigate] = useLocation();
  const containerReference = useRef(null);

  useEffect(() => {
    const container: Element | null = containerReference.current;
    if (!container) return;

    function handleClick(event: Event) {
      const target: Element = event.target! as Element;
      const anchor = target.closest('a[href^="note://"]');
      if (!anchor) return;

      event.preventDefault();
      const href = (anchor as HTMLAnchorElement).href;
      const [, id] = href.split("//");

      navigate(`/note/${id}`);
    }

    (container as Element).addEventListener("click", handleClick);

    return () => {
      (container as Element).removeEventListener("click", handleClick);
    };
  }, [navigate]);

  useEffect(() => {
    if (status === "dirty" || status === "empty") {
      fetchNoteContent(id);
    }
  }, [id, status, fetchNoteContent]);

  return (
    <div ref={containerReference}>
      {html ? (
        <div dangerouslySetInnerHTML={{ __html: html }} />
      ) : (
        <div>
          <p>TODO: Loading</p>
        </div>
      )}
    </div>
  );
}
