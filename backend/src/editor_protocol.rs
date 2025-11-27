use std::{
    convert::Infallible,
    error::Error,
    future::Future,
    io,
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
};

use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};
use tokio_util::sync::CancellationToken;
use tower::{MakeService, Service};
use uuid::Uuid;

use crate::notes_service::NoteItem;

#[derive(Debug)]
pub struct EditorServer<M> {
    listener: TcpListener,
    make_service: M,
    // TODO: Be a good person and make this a generic future
    cancel: CancellationToken,
}

impl<M> EditorServer<M> {
    pub fn new(listener: TcpListener, make_service: M, cancel: CancellationToken) -> Self {
        Self {
            listener,
            make_service,
            cancel,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct GetNotesRequest;

#[derive(Serialize, Deserialize)]
pub struct GetNotesResponse {
    pub items: Result<Vec<NoteItem>, String>,
}

#[derive(Serialize, Deserialize)]
pub struct FocusNoteRequest {
    pub id: Uuid,
}

#[derive(Serialize, Deserialize)]
pub struct FocusNoteResponse {
    pub result: Result<(), String>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "tag")]
pub enum Message<GetNotes, FocusNote> {
    #[serde(rename(serialize = "get_notes", deserialize = "get_notes"))]
    GetNotes(GetNotes),
    #[serde(rename(serialize = "focus_note", deserialize = "focus_note"))]
    FocusNote(FocusNote),
}

pub type Request = Message<GetNotesRequest, FocusNoteRequest>;

pub type Response = Message<GetNotesResponse, FocusNoteResponse>;

impl<M> EditorServer<M>
where
    M: MakeService<
            SocketAddr,
            Request,
            Response = Response,
            Error = Infallible,
            MakeError = Infallible,
        >,
    M::Service: Send + 'static,
    <M::Service as Service<Request>>::Future: Send,
{
    pub async fn run(mut self) -> Result<(), io::Error> {
        loop {
            tokio::select! {
                result = self.listener.accept() => {
                    let (socket, address) = result?;
                    let Ok(service) = self.make_service.make_service(address).await;

                    tokio::spawn(handle_socket(socket, service));
                }
                _ = self.cancel.cancelled() => {
                    break Ok(());
                }
            }
        }
    }
}

async fn handle_socket<S>(socket: TcpStream, service: S)
where
    S: Service<Request, Response = Response, Error = Infallible> + Send,
    S::Future: Send,
{
    if let Err(error) = handle_socket_helper(socket, service).await {
        match error {
            EditorHandleError::Io(error) => {
                println!("handle socket error: {:?}", error)
            }
            EditorHandleError::Serde(error) => {
                println!("handle socket error: {:?}", error)
            }
        }
    }
}

enum EditorHandleError {
    Io(io::Error),
    Serde(serde_json::Error),
}

async fn handle_socket_helper<S>(socket: TcpStream, mut service: S) -> Result<(), EditorHandleError>
where
    S: Service<Request, Response = Response, Error = Infallible> + Send,
    S::Future: Send,
{
    let mut socket = BufReader::new(socket);

    let mut buffer = String::new();
    socket
        .read_line(&mut buffer)
        .await
        .map_err(EditorHandleError::Io)?;
    let request: Request = serde_json::from_str(&buffer).map_err(EditorHandleError::Serde)?;

    let Ok(response) = service.call(request).await;

    let buffer = serde_json::to_string(&response).map_err(EditorHandleError::Serde)?;
    socket
        .write_all(buffer.as_bytes())
        .await
        .map_err(EditorHandleError::Io)?;

    Ok(())
}

pub trait Editor {
    type GetNotesError: Error;
    type GetNotesFuture: Future<Output = Result<Vec<NoteItem>, Self::GetNotesError>>;

    fn get_notes(&mut self) -> Self::GetNotesFuture;

    type FocusNoteError: Error;
    type FocusNoteFuture: Future<Output = Result<(), Self::FocusNoteError>>;

    fn focus_note(&mut self, id: Uuid) -> Self::FocusNoteFuture;
}

#[derive(Debug, Clone)]
pub struct EditorServiceWrapper<T>(pub T);

impl<T: Editor> Service<Request> for EditorServiceWrapper<T> {
    type Response = Response;
    type Error = Infallible;
    type Future =
        EditorServiceResponseFuture<T::GetNotesFuture, T::FocusNoteFuture>;

    fn poll_ready(&mut self, _context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request) -> Self::Future {
        match request {
            Message::GetNotes(GetNotesRequest) => EditorServiceResponseFuture::GetNotes(
                self.0.get_notes(),
            ),
            Message::FocusNote(FocusNoteRequest { id }) => {
                EditorServiceResponseFuture::FocusNote(self.0.focus_note(id))
            }
        }
    }
}

#[pin_project::pin_project(project = EditorServiceResponseFutureProjection)]
#[derive(Debug)]
pub enum EditorServiceResponseFuture<GetNotesFuture, FocusNoteFuture> {
    GetNotes(#[pin] GetNotesFuture),
    FocusNote(#[pin] FocusNoteFuture),
}

impl<GetNotesFuture, FocusNoteFuture, GetNotesError, FocusNoteError> Future
    for EditorServiceResponseFuture<GetNotesFuture, FocusNoteFuture>
where
    GetNotesFuture: Future<Output = Result<Vec<NoteItem>, GetNotesError>>,
    GetNotesError: Error,
    FocusNoteFuture: Future<Output = Result<(), FocusNoteError>>,
    FocusNoteError: Error,
{
    type Output = Result<Response, Infallible>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        use EditorServiceResponseFutureProjection::*;

        match self.project() {
            GetNotes(future) => future.poll(context).map(|result| {
                Ok(Response::GetNotes(GetNotesResponse {
                    items: result.map_err(|error| format!("{:?}", error)),
                }))
            }),
            FocusNote(future) => future.poll(context).map(|result| {
                Ok(Response::FocusNote(FocusNoteResponse {
                    result: result.map_err(|error| format!("{:?}", error)),
                }))
            }),
        }
    }
}
