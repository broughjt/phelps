import { JSX } from "react";
import { NoteContent } from "./NoteContent";
import { Link } from "wouter";

type NotePageProperties = {
  id: string;
  title: string;
  backlinks: Record<string, string>;
  status: "loaded" | "dirty" | "loading";
  html: string | null;
  warnings: string[];
  errors: string[];
  fetchNoteContent: (id: string) => Promise<void>;
};

export function NotePage({
  id,
  title,
  backlinks,
  status,
  html,
  warnings,
  errors,
  fetchNoteContent,
}: NotePageProperties): JSX.Element {
  // return (
  //   <div className="layout">
  //     <article>
  //       <h1>{title}</h1>
  //       <NoteContent
  //         id={id}
  //         status={status}
  //         html={html}
  //         fetchNoteContent={fetchNoteContent}
  //       />
  //     </article>
  //     <aside>
  //       <h2>Backlinks</h2>
  //       <ul>
  //         {Object.entries(backlinks).map(([id, title]) => (
  //           <li key={id}>
  //             <a href={`/note/${id}`}>{title}</a>
  //           </li>
  //         ))}
  //       </ul>
  //     </aside>
  //     <div className="right-column"></div>
  //   </div>
  // );

  return (
    <div className="layout">
      <main>
        <h1>{title}</h1>
        <NoteContent
          id={id}
          status={status}
          html={html}
          warnings={warnings}
          errors={errors}
          fetchNoteContent={fetchNoteContent}
        />
        {Object.keys(backlinks).length > 0 ? (
          <>
            <h3>Backlinks</h3>
            <ul>
              {Object.entries(backlinks).map(([id, title]) => (
                <li key={id}>
                  <Link to={`note/${id}`}>{title}</Link>
                </li>
              ))}
            </ul>
          </>
        ) : (
          <></>
        )}
      </main>
    </div>
  );
}
