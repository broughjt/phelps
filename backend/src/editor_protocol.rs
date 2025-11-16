use std::{
    convert::Infallible,
    io,
    net::SocketAddr,
    path::PathBuf,
    pin::Pin,
    task::{Context, Poll},
};

use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tokio_util::sync::CancellationToken;
use tower::{MakeService, Service};
use uuid::Uuid;

pub struct Server<M> {
    listener: TcpListener,
    make_service: M,
    // TODO: Be a good person and make this a generic future
    cancel: CancellationToken,
}

impl<M> Server<M> {
    pub fn new(listener: TcpListener, make_service: M, cancel: CancellationToken) -> Self {
        Self {
            listener,
            make_service,
            cancel,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct NoteItem {
    pub id: Uuid,
    pub title: String,
    pub path: PathBuf,
}

pub type GetNotesRequest = ();

#[derive(Serialize, Deserialize)]
pub struct GetNotesResponse {
    items: Vec<NoteItem>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "tag", content = "content")]
pub enum Message<G> {
    #[serde(rename(serialize = "get_notes", deserialize = "get_notes"))]
    GetNotes(G),
}

type Request = Message<GetNotesRequest>;

type Response = Message<GetNotesResponse>;

impl<M> Server<M>
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
    let _ = handle_socket_helper(socket, service).await;
}

enum Error {
    Io,
    Serde,
}

async fn handle_socket_helper<S>(mut socket: TcpStream, mut service: S) -> Result<(), Error>
where
    S: Service<Request, Response = Response, Error = Infallible> + Send,
    S::Future: Send,
{
    let mut buffer = String::new();
    socket
        .read_to_string(&mut buffer)
        .await
        .map_err(|_| Error::Io)?;
    let request: Request = serde_json::from_str(&buffer).map_err(|_| Error::Serde)?;

    let Ok(response) = service.call(request).await;

    let buffer = serde_json::to_string(&response).map_err(|_| Error::Serde)?;
    socket
        .write_all(buffer.as_bytes())
        .await
        .map_err(|_| Error::Io)?;

    Ok(())
}

pub trait Editor {
    type GetNotesError;
    type GetNotesFuture: Future<Output = Result<Vec<NoteItem>, Self::GetNotesError>>;

    fn get_notes(&mut self) -> Self::GetNotesFuture;
}

pub struct EditorService<T>(T);

impl<T: Editor> Service<Request> for EditorService<T> {
    type Response = Response;
    type Error = Message<T::GetNotesError>;
    type Future = EditorServiceResponseFuture<T::GetNotesFuture>;

    fn poll_ready(&mut self, _context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request) -> Self::Future {
        match request {
            Message::GetNotes(()) => EditorServiceResponseFuture::GetNotes(self.0.get_notes()),
        }
    }
}

#[pin_project::pin_project(project = EditorServiceResponseFutureProjection)]
#[derive(Debug)]
pub enum EditorServiceResponseFuture<GetNotesFuture> {
    GetNotes(#[pin] GetNotesFuture),
}

impl<GetNotesFuture, GetNotesError> Future for EditorServiceResponseFuture<GetNotesFuture>
where
    GetNotesFuture: Future<Output = Result<Vec<NoteItem>, GetNotesError>>,
{
    type Output = Result<Response, Message<GetNotesError>>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        use EditorServiceResponseFutureProjection::*;

        match self.project() {
            GetNotes(future) => future
                .poll(context)
                .map_ok(|items| Message::GetNotes(GetNotesResponse { items }))
                .map_err(Message::GetNotes),
        }
    }
}
