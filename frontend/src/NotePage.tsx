import { useParams } from "wouter";
import "./NotePage.css";
import { NotesApi } from "./api";
import type { NoteMetadata } from "./api";
import { useEffect, useState } from "react";

const api = new NotesApi("http://localhost:3000");

export default function NotePage() {
  const { id } = useParams();
  const [content, setContent] = useState<string>("")
  const [metadata, setMetadata] = useState<NoteMetadata | null>(null)

  useEffect(() => {
    if (!id) return

    api.getNoteContent(id).then(setContent).catch(console.error)
    api.getNoteMetadata(id).then(setMetadata).catch(console.error)
  }, [id])

  return (
    <div className="layout">
      <article dangerouslySetInnerHTML={{ __html: content }}></article>
      <aside>
        <ul>
          {metadata && metadata.links.map(l => <li key={l}>{l}</li>)}
        </ul>
      </aside>
      <div className="right-column"></div>
    </div>
  )
}
