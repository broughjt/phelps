import { JSX, memo } from "react";
import "./NotePage.css";

type NotePageProps = {
  id: string;
  title: string;
  links: Set<string>;
  backlinks: Set<string>;
};

function NotePage({ title, backlinks }: NotePageProps): JSX.Element {
  return (
    <div className="layout">
      <article>
        <h1>{title}</h1>
      </article>
      <aside>
        <h2>Backlinks</h2>
        <ul>
          {Array.from(backlinks).map((backlink) => (
            <li key={backlink}>
              <a href={`/note/${backlink}`}>{backlink}</a>
            </li>
          ))}
        </ul>
      </aside>
      <div className="right-column"></div>
    </div>
  );
}

export default memo(NotePage);
