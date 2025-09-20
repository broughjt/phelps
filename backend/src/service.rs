use std::collections::HashMap;

use petgraph::{graph::NodeIndex, stable_graph::{DefaultIx, StableGraph}, Direction};
use serde_derive::Serialize;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

pub const CONTENT1: &str = include_str!("../test1.html");
pub const CONTENT2: &str = include_str!("../test2.html");
pub const CONTENT3: &str = include_str!("../test3.html");

pub const BUFFER_SIZE: usize = 64;

struct NotesActor {
    links: StableGraph<Uuid, ()>,
    node_ids: HashMap<Uuid, NodeIndex<DefaultIx>>,
    titles: HashMap<Uuid, String>,
}

impl Default for NotesActor {
    fn default() -> Self {
        let uuid1 = Uuid::try_parse("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let uuid2 = Uuid::try_parse("550e8400-e29b-41d4-a716-446655440001").unwrap();
        let uuid3 = Uuid::try_parse("550e8400-e29b-41d4-a716-446655440002").unwrap();

        let mut links = StableGraph::new();
        let node1 = links.add_node(uuid1);
        let node2 = links.add_node(uuid2);
        let node3 = links.add_node(uuid3);
        links.extend_with_edges([
            (node1, node2),
            (node1, node3),
            (node2, node3),
            (node3, node1),
        ]);

        let node_ids = HashMap::from_iter([
            (uuid1, node1),
            (uuid2, node2),
            (uuid3, node3),
        ]);

        let titles = HashMap::from_iter([
            (uuid1, "Note 1".into()),
            (uuid2, "Note 2".into()),
            (uuid3, "Note 3".into()),
        ]);

        Self { links, node_ids, titles }
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
    pub result: Result<Option<String>, ()>,
}

struct GetNoteMetadataRequest {
    pub id: Uuid,
}

struct GetNoteMetadataResponse {
    pub result: Option<NoteMetadata>,
}

#[derive(Debug, Serialize)]
pub struct NoteMetadata {
    title: String, // TODO
    links: Vec<Uuid>,
    backlinks: Vec<Uuid>,
}

impl Message<GetNoteContentRequest> for NotesActor {
    type Response = GetNoteContentResponse;

    async fn handle(&mut self, GetNoteContentRequest { id }: GetNoteContentRequest) -> Self::Response {
        if id == Uuid::try_parse("550e8400-e29b-41d4-a716-446655440000").unwrap() {
            GetNoteContentResponse { result: Ok(Some(CONTENT1.into())) }
        } else if id == Uuid::try_parse("550e8400-e29b-41d4-a716-446655440001").unwrap() {
            GetNoteContentResponse { result: Ok(Some(CONTENT2.into())) }
        } else if id == Uuid::try_parse("550e8400-e29b-41d4-a716-446655440001").unwrap() {
            GetNoteContentResponse { result: Ok(Some(CONTENT3.into())) }
        } else {
            GetNoteContentResponse { result: Ok(None) }
        }
    }
}

impl Message<GetNoteMetadataRequest> for NotesActor {
    type Response = GetNoteMetadataResponse;

    async fn handle(&mut self, GetNoteMetadataRequest { id }: GetNoteMetadataRequest) -> Self::Response {
        let result = self.node_ids.get(&id).map(|i| {
            let title = self.titles[&id].clone();
            let links = self.links.neighbors_directed(*i, Direction::Outgoing).map(|i| self.links[i]).collect();
            let backlinks = self.links.neighbors_directed(*i, Direction::Incoming).map(|i| self.links[i]).collect();

            NoteMetadata { title, links, backlinks }
        });

        GetNoteMetadataResponse { result }
    }
}


enum NotesMessage {
    GetNoteContent(GetNoteContentRequest, oneshot::Sender<GetNoteContentResponse>),
    GetNoteMetadata(GetNoteMetadataRequest, oneshot::Sender<GetNoteMetadataResponse>)
}

#[derive(Clone, Debug)]
pub struct NotesActorHandle {
    sender: mpsc::Sender<NotesMessage>
}

impl From<mpsc::Sender<NotesMessage>> for NotesActorHandle {
    fn from(sender: mpsc::Sender<NotesMessage>) -> Self {
        Self { sender }
    }
}

#[derive(Debug, Error)]
pub enum NotesActorHandleError {
    #[error("send error")]
    Send,
    #[error("receive error")]
    Receive
}

impl NotesActorHandle {
    pub async fn get_note_content(&self, id: Uuid) -> Result<Result<Option<String>, ()>, NotesActorHandleError> {
        let (sender, receiver) = oneshot::channel();
        let message = NotesMessage::GetNoteContent(GetNoteContentRequest { id }, sender);
        self.sender.send(message).await.map_err(|_| NotesActorHandleError::Send)?;
        let GetNoteContentResponse { result } = receiver.await.map_err(|_| NotesActorHandleError::Receive)?;

        Ok(result)
    }

    pub async fn get_note_metadata(&self, id: Uuid) -> Result<Option<NoteMetadata>, NotesActorHandleError> {
        let (sender, receiver) = oneshot::channel();
        let message = NotesMessage::GetNoteMetadata(GetNoteMetadataRequest { id }, sender);
        self.sender.send(message).await.map_err(|_| NotesActorHandleError::Send)?;
        let GetNoteMetadataResponse { result } = receiver.await.map_err(|_| NotesActorHandleError::Receive)?;

        Ok(result)
    }

    pub fn spawn() -> NotesActorHandle {
        let mut state = NotesActor::default();
        let (sender, mut receiver) = mpsc::channel(BUFFER_SIZE);

        // TODO: Take a shutdown signal
        tokio::spawn(async move {
            while let Some(message) = receiver.recv().await {
                match message {
                    NotesMessage::GetNoteContent(request, sender) => {
                        let response = state.handle(request).await;
                        let _ = sender.send(response);
                    },
                    NotesMessage::GetNoteMetadata(request, sender) => {
                        let response = state.handle(request).await;
                        let _ = sender.send(response);
                    }
                }
            }
        });

        sender.into()
    }
}
