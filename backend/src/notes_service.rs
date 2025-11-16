use std::{collections::HashMap, io, path::PathBuf, sync::Arc};

use petgraph::prelude::DiGraphMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{
    fs,
    sync::{broadcast, mpsc, oneshot},
};
use tokio_util::sync::CancellationToken;
use typst::{
    diag::{SourceDiagnostic, Warned},
    ecow::EcoVec,
    syntax::FileId,
};
use uuid::Uuid;

use crate::event::Event;

struct NotesServiceState {
    cancel: CancellationToken,
    links: DiGraphMap<Uuid, ()>,
    build_subdirectory: PathBuf,
    titles: HashMap<Uuid, String>,
    file_ids: HashMap<Uuid, FileId>,
    ids: HashMap<FileId, Vec<Uuid>>,
    errors: HashMap<FileId, Result<Warned<()>, EcoVec<SourceDiagnostic>>>,
    build_finished_event: Arc<Event>,
    updates: broadcast::Sender<NoteUpdate>,
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

    fn create_note(
        &mut self,
        file_id: FileId,
        NoteData {
            title,
            id: i,
            links,
        }: NoteData,
    ) {
        self.links.add_node(i);

        for j in links {
            self.links.add_edge(i, j, ());
        }

        self.ids.get_mut(&file_id).unwrap().push(i);
        self.titles.insert(i, title);
        self.file_ids.insert(i, file_id);
    }

    fn create_notes(
        &mut self,
        file_id: FileId,
        result: Result<Warned<Vec<NoteData>>, EcoVec<SourceDiagnostic>>,
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
                self.ids.insert(file_id, Vec::with_capacity(output.len()));

                for data in output.iter().cloned() {
                    self.create_note(file_id, data);
                }

                if self.build_finished_event.has_occured() && !output.is_empty() {
                    let _ = self.updates.send(NoteUpdate::Update(output));
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
        NoteData {
            title,
            id: i,
            links,
        }: NoteData,
    ) {
        let js: Vec<Uuid> = self.links.neighbors(i).collect();

        for j in js {
            self.links.remove_edge(i, j);
        }
        for j in links {
            self.links.add_edge(i, j, ());
        }

        self.ids.get_mut(&file_id).unwrap().push(i);
        self.titles.insert(i, title);
        self.file_ids.insert(i, file_id);
    }

    fn update_notes(
        &mut self,
        updates: Vec<(
            FileId,
            Result<Warned<Vec<NoteData>>, EcoVec<SourceDiagnostic>>,
        )>,
    ) {
        let mut data: Vec<NoteData> = Vec::new();

        for (file_id, result) in updates {
            match result {
                Ok(Warned { output, warnings }) => {
                    self.errors.insert(
                        file_id,
                        Ok(Warned {
                            output: (),
                            warnings,
                        }),
                    );
                    self.ids.get_mut(&file_id).unwrap().clear();

                    data.extend(output.iter().cloned());

                    for data in output {
                        self.update_note(file_id, data);
                    }
                }
                Err(error) => {
                    self.errors.insert(file_id, Err(error));
                }
            }
        }

        if self.build_finished_event.has_occured() && !data.is_empty() {
            let _ = self.updates.send(NoteUpdate::Update(data));
        }
    }

    async fn remove_notes(&mut self, file_id: FileId) {
        self.errors.remove(&file_id);
        if let Some(is) = self.ids.remove(&file_id) {
            for i in is.iter() {
                self.titles.remove(&i);
                self.file_ids.remove(&i);
                self.links.remove_node(*i);
            }

            let removes = is.iter().map(|i| {
                let path = self.build_subdirectory.join(format!("{}.html", i));
                fs::remove_file(path)
            });

            if let Err(error) = futures::future::join_all(removes)
                .await
                .into_iter()
                .try_for_each(|result| result)
            {
                // Failed to remove fragments from build directory. This is
                // fatal, so we need to tell the rest of the application to
                // shutdown.

                println!(
                    "Failed to remove fragments from the build directory {}",
                    error
                );
                self.cancel.cancel();
            }

            let _ = self.updates.send(NoteUpdate::Remove(is));
        }
    }

    // TODO: We need all three
    fn set_build_finished(&mut self) {
        self.build_finished_event.trigger();
    }

    fn get_build_finished(&mut self) -> Arc<Event> {
        self.build_finished_event.clone()
    }

    fn subscribe(&mut self) -> (Initialize, broadcast::Receiver<NoteUpdate>) {
        let mut outgoing_links: HashMap<Uuid, Vec<Uuid>> =
            HashMap::with_capacity(self.links.node_count());

        for (u, v, _) in self.links.all_edges() {
            outgoing_links.entry(u).or_default().push(v);
        }

        let initialize = Initialize {
            outgoing_links,
            titles: self.titles.clone(),
        };

        (initialize, self.updates.subscribe())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NoteData {
    pub title: String,
    pub id: Uuid,
    pub links: Vec<Uuid>,
}

#[derive(Serialize, Deserialize)]
pub struct Initialize {
    pub outgoing_links: HashMap<Uuid, Vec<Uuid>>,
    pub titles: HashMap<Uuid, String>,
}

#[derive(Clone)]
pub enum NoteUpdate {
    Update(Vec<NoteData>),
    Remove(Vec<Uuid>),
}

enum NotesMessage {
    GetNoteContent(Uuid, oneshot::Sender<Result<Option<String>, io::Error>>),
    CreateNotes(
        FileId,
        Result<Warned<Vec<NoteData>>, EcoVec<SourceDiagnostic>>,
    ),
    UpdateNotes(
        Vec<(
            FileId,
            Result<Warned<Vec<NoteData>>, EcoVec<SourceDiagnostic>>,
        )>,
    ),
    RemoveNotes(FileId),
    SetBuildFinished,
    GetBuildFinished(oneshot::Sender<Arc<Event>>),
    Subscribe(oneshot::Sender<(Initialize, broadcast::Receiver<NoteUpdate>)>),
}

pub struct NotesService {
    receiver: mpsc::Receiver<NotesMessage>,
    state: NotesServiceState,
}

impl NotesService {
    async fn handle(&mut self, message: NotesMessage) {
        match message {
            NotesMessage::GetNoteContent(uuid, sender) => {
                let response = self.state.get_note_content(uuid).await;
                let _ = sender.send(response);
            }
            NotesMessage::CreateNotes(file_id, result) => {
                self.state.create_notes(file_id, result);
            }
            NotesMessage::UpdateNotes(updates) => {
                self.state.update_notes(updates);
            }
            NotesMessage::RemoveNotes(file_id) => {
                self.state.remove_notes(file_id).await;
            }
            NotesMessage::SetBuildFinished => {
                self.state.set_build_finished();
            }
            NotesMessage::GetBuildFinished(sender) => {
                let event = self.state.get_build_finished();
                let _ = sender.send(event);
            }
            NotesMessage::Subscribe(sender) => {
                let result = self.state.subscribe();
                let _ = sender.send(result);
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
                _ = self.state.cancel.cancelled() => {
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
        let message = NotesMessage::GetNoteContent(id, sender);
        self.sender
            .send(message)
            .await
            .map_err(|_| NotesServiceHandleError::Send)?;
        let result = receiver
            .await
            .map_err(|_| NotesServiceHandleError::Receive)?;

        Ok(result)
    }

    pub async fn create_notes(
        &self,
        file_id: FileId,
        result: Result<Warned<Vec<NoteData>>, EcoVec<SourceDiagnostic>>,
    ) -> Result<(), NotesServiceHandleError> {
        let message = NotesMessage::CreateNotes(file_id, result);
        self.sender
            .send(message)
            .await
            .map_err(|_| NotesServiceHandleError::Send)?;

        Ok(())
    }

    pub async fn update_notes(
        &self,
        updates: Vec<(
            FileId,
            Result<Warned<Vec<NoteData>>, EcoVec<SourceDiagnostic>>,
        )>,
    ) -> Result<(), NotesServiceHandleError> {
        let message = NotesMessage::UpdateNotes(updates);
        self.sender
            .send(message)
            .await
            .map_err(|_| NotesServiceHandleError::Send)?;

        Ok(())
    }

    pub async fn remove_notes(&self, file_id: FileId) -> Result<(), NotesServiceHandleError> {
        let message = NotesMessage::RemoveNotes(file_id);
        self.sender
            .send(message)
            .await
            .map_err(|_| NotesServiceHandleError::Send)?;

        Ok(())
    }

    pub async fn set_build_finished(&self) -> Result<(), NotesServiceHandleError> {
        self.sender
            .send(NotesMessage::SetBuildFinished)
            .await
            .map_err(|_| NotesServiceHandleError::Send)?;

        Ok(())
    }

    pub async fn get_build_finished(&self) -> Result<Arc<Event>, NotesServiceHandleError> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(NotesMessage::GetBuildFinished(sender))
            .await
            .map_err(|_| NotesServiceHandleError::Send)?;

        receiver.await.map_err(|_| NotesServiceHandleError::Receive)
    }

    pub async fn subscribe(
        &self,
    ) -> Result<(Initialize, broadcast::Receiver<NoteUpdate>), NotesServiceHandleError> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(NotesMessage::Subscribe(sender))
            .await
            .map_err(|_| NotesServiceHandleError::Send)?;

        receiver.await.map_err(|_| NotesServiceHandleError::Receive)
    }

    pub fn build(
        cancel: CancellationToken,
        build_subdirectory: PathBuf,
    ) -> (NotesServiceHandle, NotesService) {
        pub const BUFFER_SIZE: usize = 64;

        let (sender, receiver) = mpsc::channel(BUFFER_SIZE);
        let (updates, _) = broadcast::channel(BUFFER_SIZE);
        let state = NotesServiceState {
            cancel: cancel,
            build_subdirectory,
            links: DiGraphMap::default(),
            ids: HashMap::default(),
            titles: HashMap::default(),
            file_ids: HashMap::default(),
            errors: HashMap::default(),
            build_finished_event: Event::new(),
            updates,
        };
        let service = NotesService { state, receiver };
        let handle = NotesServiceHandle { sender };

        (handle, service)
    }
}
