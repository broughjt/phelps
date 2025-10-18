export interface NoteMetadata {
  title: string
  links: string[]
  backlinks: string[]
}

export class NotesApi {
  private url: string

  constructor(url: string) {
    this.url = url
  }

  async getNoteContent(id: string): Promise<string> {
    const response = await fetch(`${this.url}/api/notes/${id}/content`)
    if (response.status != 200) {
      throw new Error(`Failed to fetch note content: status ${response.status}`)
    }

    const contentType = response.headers.get("Content-Type") ?? "";
    if (!contentType.includes("text/html")) {
      throw new Error(`Unexpected content type: ${contentType}`)
    }

    return await response.text()
  }

  async getNoteMetadata(id: String): Promise<NoteMetadata | null> {
    const response = await fetch(`${this.url}/api/notes/${id}/metadata`)
    if (response.status != 200) {
      throw new Error(`Failed to fetch note metadata: status ${response.status}`)
    }

    const contentType = response.headers.get("Content-Type") ?? "";
    if (contentType != "application/json") {
      throw new Error(`Unexpected content type: ${contentType}`)
    }

    return await response.json() as NoteMetadata
  }
}
