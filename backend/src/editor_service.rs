use std::pin::Pin;

use crate::{
    editor_protocol::Editor,
    notes_service::{NoteItem, NotesServiceHandle, NotesServiceHandleError},
};

#[derive(Clone, Debug)]
pub struct EditorService {
    notes_service: NotesServiceHandle,
}

impl EditorService {
    pub fn new(notes_service: NotesServiceHandle) -> Self {
        Self { notes_service }
    }
}

impl Editor for EditorService {
    type GetNotesError = NotesServiceHandleError;
    type GetNotesFuture =
        Pin<Box<dyn Future<Output = Result<Vec<NoteItem>, Self::GetNotesError>> + Send>>;

    fn get_notes(&mut self) -> Self::GetNotesFuture {
        let notes_service = self.notes_service.clone();
        let future = async move { notes_service.get_notes().await };

        Box::pin(future)
    }

    type FocusNoteError = NotesServiceHandleError;
    type FocusNoteFuture = Pin<Box<dyn Future<Output = Result<(), Self::FocusNoteError>> + Send>>;

    fn focus_note(&mut self, id: uuid::Uuid) -> Self::FocusNoteFuture {
        let notes_service = self.notes_service.clone();
        let future = async move { notes_service.focus_note(id).await };

        Box::pin(future)
    }
}
