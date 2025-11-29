import { JSX, useEffect, useRef } from "react";
import { useLocation } from "wouter";

type NoteContentProperties = {
  id: string;
  status: "empty" | "loaded" | "dirty" | "loading";
  html: string | null;
  warnings: string[];
  errors: string[];
  fetchNoteContent: (id: string) => Promise<void>;
};

export function NoteContent({
  id,
  status,
  html,
  warnings,
  errors,
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

  if (errors.length > 0) {
    return (
      <div ref={containerReference}>
        <h3>Errors</h3>
        <ul>
          {errors.map((error, index) => (
            <li key={index}>{error}</li>
          ))}
        </ul>
        <h3>Warnings</h3>
        <ul>
          {warnings.map((warning, index) => (
            <li key={index}>{warning}</li>
          ))}
        </ul>
      </div>
    );
  } else if (html) {
    return (
      <div ref={containerReference}>
        <div dangerouslySetInnerHTML={{ __html: html }} />
      </div>
    );
  } else {
    return (
      <div ref={containerReference}>
        <p>TODO: Loading</p>
      </div>
    );
  }
}
