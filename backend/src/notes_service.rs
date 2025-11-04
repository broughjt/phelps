use std::{collections::HashMap, io, path::PathBuf};

use petgraph::{Direction, prelude::DiGraphMap};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{fs, sync::{mpsc, oneshot, broadcast}};
use tokio_util::sync::CancellationToken;
use typst::{
    diag::{SourceDiagnostic, Warned},
    ecow::EcoVec,
    syntax::FileId,
};
use uuid::Uuid;

pub const BUFFER_SIZE: usize = 64;

// - We shouldn't send any websocket updates until the build server has compiled
//   all the notes initially and we have an entire graph to send

struct NotesServiceState {
    links: DiGraphMap<Uuid, ()>,
    build_subdirectory: PathBuf,
    metadata: HashMap<Uuid, (String, FileId)>,
    file_ids: HashMap<FileId, Vec<Uuid>>,
    errors: HashMap<FileId, Result<Warned<()>, EcoVec<SourceDiagnostic>>>,
    broadcast::Sender<_>
}

impl NotesServiceState {
    async fn get_note_content(&mut self, id: Uuid) -> Result<Option<String>, io::Error> {
        if self.links.contains_node(id) {
            let path = self.build_subdirectory.join(format!("{}.html", id));

            let content = fs::read_to_string(path).await?;

            Ok(Some(content))
        } else {
            Ok(None)
        }
    }

    fn get_note_metadata(&mut self, id: Uuid) -> Option<GetNoteMetadata> {
        if self.links.contains_node(id) {
            let (title, _) = &self.metadata[&id];
            let links = self
                .links
                .neighbors_directed(id, Direction::Outgoing)
                .collect();
            let backlinks = self
                .links
                .neighbors_directed(id, Direction::Incoming)
                .collect();

            Some(GetNoteMetadata {
                title: title.clone(),
                links,
                backlinks,
            })
        } else {
            None
        }
    }

    fn create_note(
        &mut self,
        file_id: FileId,
        CreateNoteMetadata {
            title,
            id: i,
            links,
        }: CreateNoteMetadata,
    ) {
        self.links.add_node(i);

        for j in links {
            self.links.add_edge(i, j, ());
        }

        self.file_ids.get_mut(&file_id).unwrap().push(i);
        self.metadata.insert(i, (title, file_id));
    }

    fn create_notes(
        &mut self,
        file_id: FileId,
        result: Result<Warned<Vec<CreateNoteMetadata>>, EcoVec<SourceDiagnostic>>,
    ) {
        match result {
            Ok(Warned { output, warnings }) => {
                self.errors.insert(
                    file_id,
                    Ok(Warned {
                        output: (),
                        warnings,
                    }),
                );
                self.file_ids
                    .insert(file_id, Vec::with_capacity(output.len()));

                for data in output {
                    self.create_note(file_id, data);
                }
            }
            Err(error) => {
                self.errors.insert(file_id, Err(error));
            }
        }
    }

    fn update_note(
        &mut self,
        file_id: FileId,
        CreateNoteMetadata {
            title,
            id: i,
            links,
        }: CreateNoteMetadata,
    ) {
        let js: Vec<Uuid> = self.links.neighbors(i).collect();

        for j in js {
            self.links.remove_edge(i, j);
        }
        for j in links {
            self.links.add_edge(i, j, ());
        }

        self.file_ids.get_mut(&file_id).unwrap().push(i);
        self.metadata.insert(i, (title, file_id));
    }

    fn update_notes(
        &mut self,
        outputs: Vec<(
            FileId,
            Result<Warned<Vec<CreateNoteMetadata>>, EcoVec<SourceDiagnostic>>,
        )>,
    ) {
        for (file_id, result) in outputs {
            match result {
                Ok(Warned { output, warnings }) => {
                    self.errors.insert(
                        file_id,
                        Ok(Warned {
                            output: (),
                            warnings,
                        }),
                    );
                    self.file_ids.get_mut(&file_id).unwrap().clear();

                    for data in output {
                        self.update_note(file_id, data);
                    }
                }
                Err(error) => {
                    self.errors.insert(file_id, Err(error));
                }
            }
        }
    }

    fn remove_notes(&mut self, file_id: FileId) {
        self.errors.remove(&file_id);
        if let Some(is) = self.file_ids.remove(&file_id) {
            for i in is {
                self.metadata.remove(&i);
                self.links.remove_node(i);
            }
        }
    }
}

trait Message<T> {
    type Response;

    async fn handle(&mut self, request: T) -> Self::Response;
}

struct GetNoteContentRequest {
    pub id: Uuid,
}

struct GetNoteContentResponse {
    pub result: Result<Option<String>, io::Error>,
}

struct GetNoteMetadataRequest {
    pub id: Uuid,
}

struct GetNoteMetadataResponse {
    pub result: Option<GetNoteMetadata>,
}

pub struct CreateNoteMetadata {
    pub title: String,
    pub id: Uuid,
    pub links: Vec<Uuid>,
}

#[derive(Deserialize, Serialize)]
pub struct GetNoteMetadata {
    pub title: String,
    pub links: Vec<Uuid>,
    pub backlinks: Vec<Uuid>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "t", content = "c")]
pub enum NoteUpdate {
    Update
}

pub struct CreateNotesRequest {
    pub file_id: FileId,
    pub result: Result<Warned<Vec<CreateNoteMetadata>>, EcoVec<SourceDiagnostic>>,
}

pub struct CreateNotesResponse {}

pub struct UpdateNotesRequest {
    pub outputs: Vec<(
        FileId,
        Result<Warned<Vec<CreateNoteMetadata>>, EcoVec<SourceDiagnostic>>,
    )>,
}

pub struct UpdateNotesResponse {}

pub struct RemoveNotesRequest {
    pub file_id: FileId,
}

pub struct RemoveNotesResponse {}

impl Message<GetNoteContentRequest> for NotesServiceState {
    type Response = GetNoteContentResponse;

    async fn handle(
        &mut self,
        GetNoteContentRequest { id }: GetNoteContentRequest,
    ) -> Self::Response {
        let result = self.get_note_content(id).await;

        GetNoteContentResponse { result }
    }
}

impl Message<GetNoteMetadataRequest> for NotesServiceState {
    type Response = GetNoteMetadataResponse;

    async fn handle(
        &mut self,
        GetNoteMetadataRequest { id }: GetNoteMetadataRequest,
    ) -> Self::Response {
        let result = self.get_note_metadata(id);

        GetNoteMetadataResponse { result }
    }
}

impl Message<CreateNotesRequest> for NotesServiceState {
    type Response = CreateNotesResponse;

    async fn handle(
        &mut self,
        CreateNotesRequest { file_id, result }: CreateNotesRequest,
    ) -> Self::Response {
        self.create_notes(file_id, result);

        CreateNotesResponse {}
    }
}

impl Message<RemoveNotesRequest> for NotesServiceState {
    type Response = RemoveNotesResponse;

    async fn handle(
        &mut self,
        RemoveNotesRequest { file_id }: RemoveNotesRequest,
    ) -> Self::Response {
        self.remove_notes(file_id);

        RemoveNotesResponse {}
    }
}

impl Message<UpdateNotesRequest> for NotesServiceState {
    type Response = UpdateNotesResponse;

    async fn handle(
        &mut self,
        UpdateNotesRequest { outputs }: UpdateNotesRequest,
    ) -> Self::Response {
        self.update_notes(outputs);

        UpdateNotesResponse {}
    }
}

enum NotesMessage {
    GetNoteContent(
        GetNoteContentRequest,
        oneshot::Sender<GetNoteContentResponse>,
    ),
    GetNoteMetadata(
        GetNoteMetadataRequest,
        oneshot::Sender<GetNoteMetadataResponse>,
    ),
    CreateNotes(CreateNotesRequest),
    UpdateNotes(UpdateNotesRequest),
    RemoveNotes(RemoveNotesRequest),
}

pub struct NotesService {
    receiver: mpsc::Receiver<NotesMessage>,
    state: NotesServiceState,
    cancel: CancellationToken,
}

impl NotesService {
    async fn handle(&mut self, message: NotesMessage) {
        match message {
            NotesMessage::GetNoteContent(request, sender) => {
                let response = self.state.handle(request).await;
                let _ = sender.send(response);
            }
            NotesMessage::GetNoteMetadata(request, sender) => {
                let response = self.state.handle(request).await;
                let _ = sender.send(response);
            }
            NotesMessage::CreateNotes(request) => {
                self.state.handle(request).await;
            }
            NotesMessage::UpdateNotes(request) => {
                self.state.handle(request).await;
            }
            NotesMessage::RemoveNotes(request) => {
                self.state.handle(request).await;
            }
        }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                option = self.receiver.recv() => if let Some(message) = option {
                    self.handle(message).await;
                } else {
                    break
                },
                _ = self.cancel.cancelled() => {
                    println!("Notes service cancel");
                    self.receiver.close();

                    while let Some(message) = self.receiver.recv().await {
                        self.handle(message).await;
                    }

                    break
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct NotesServiceHandle {
    sender: mpsc::Sender<NotesMessage>,
}

impl From<mpsc::Sender<NotesMessage>> for NotesServiceHandle {
    fn from(sender: mpsc::Sender<NotesMessage>) -> Self {
        Self { sender }
    }
}

#[derive(Debug, Error)]
pub enum NotesServiceHandleError {
    #[error("send error")]
    Send,
    #[error("receive error")]
    Receive,
}

impl NotesServiceHandle {
    pub async fn get_note_content(
        &self,
        id: Uuid,
    ) -> Result<Result<Option<String>, io::Error>, NotesServiceHandleError> {
        let (sender, receiver) = oneshot::channel();
        let message = NotesMessage::GetNoteContent(GetNoteContentRequest { id }, sender);
        self.sender
            .send(message)
            .await
            .map_err(|_| NotesServiceHandleError::Send)?;
        let GetNoteContentResponse { result } = receiver
            .await
            .map_err(|_| NotesServiceHandleError::Receive)?;

        Ok(result)
    }

    pub async fn get_note_metadata(
        &self,
        id: Uuid,
    ) -> Result<Option<GetNoteMetadata>, NotesServiceHandleError> {
        let (sender, receiver) = oneshot::channel();
        let message = NotesMessage::GetNoteMetadata(GetNoteMetadataRequest { id }, sender);
        self.sender
            .send(message)
            .await
            .map_err(|_| NotesServiceHandleError::Send)?;
        let GetNoteMetadataResponse { result } = receiver
            .await
            .map_err(|_| NotesServiceHandleError::Receive)?;

        Ok(result)
    }

    pub async fn create_notes(
        &self,
        file_id: FileId,
        result: Result<Warned<Vec<CreateNoteMetadata>>, EcoVec<SourceDiagnostic>>,
    ) -> Result<(), NotesServiceHandleError> {
        let message = NotesMessage::CreateNotes(CreateNotesRequest { file_id, result });
        self.sender
            .send(message)
            .await
            .map_err(|_| NotesServiceHandleError::Send)?;

        Ok(())
    }

    pub async fn update_notes(
        &self,
        outputs: Vec<(
            FileId,
            Result<Warned<Vec<CreateNoteMetadata>>, EcoVec<SourceDiagnostic>>,
        )>,
    ) -> Result<(), NotesServiceHandleError> {
        let message = NotesMessage::UpdateNotes(UpdateNotesRequest { outputs });
        self.sender
            .send(message)
            .await
            .map_err(|_| NotesServiceHandleError::Send)?;

        Ok(())
    }

    pub async fn remove_notes(&self, file_id: FileId) -> Result<(), NotesServiceHandleError> {
        let message = NotesMessage::RemoveNotes(RemoveNotesRequest { file_id });
        self.sender
            .send(message)
            .await
            .map_err(|_| NotesServiceHandleError::Send)?;

        Ok(())
    }

    pub fn build(
        cancel: CancellationToken,
        build_subdirectory: PathBuf,
    ) -> (NotesServiceHandle, NotesService) {
        let (sender, receiver) = mpsc::channel(BUFFER_SIZE);
        let state = NotesServiceState {
            build_subdirectory,
            links: DiGraphMap::default(),
            metadata: HashMap::default(),
            file_ids: HashMap::default(),
            errors: HashMap::default(),
        };
        let service = NotesService {
            state,
            receiver,
            cancel,
        };
        let handle = NotesServiceHandle { sender };

        (handle, service)
    }
}
