use std::io;

use axum::{
    Router,
    body::Body,
    extract::{
        Path, State,
        ws::{self, Message, WebSocket},
    },
    response::{Html, IntoResponse},
    routing::{any, get},
};
use http::{Response, StatusCode};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::broadcast;
use tower_http::cors;
use uuid::Uuid;

use crate::notes_service::{
    Initialize, NoteData, NoteUpdate, NotesServiceHandle, NotesServiceHandleError,
};

struct GetNoteContentResponse {
    result: Result<Result<Option<String>, io::Error>, NotesServiceHandleError>,
}

impl IntoResponse for GetNoteContentResponse {
    fn into_response(self) -> Response<Body> {
        match self.result {
            Ok(Ok(Some(content))) => IntoResponse::into_response(Html(content)),
            Ok(Ok(None)) => IntoResponse::into_response(StatusCode::NOT_FOUND),
            _ => IntoResponse::into_response(StatusCode::INTERNAL_SERVER_ERROR),
        }
    }
}

async fn get_note_content(
    State(notes_service): State<NotesServiceHandle>,
    Path(id): Path<Uuid>,
) -> GetNoteContentResponse {
    let result = notes_service.get_note_content(id).await;

    GetNoteContentResponse { result }
}

#[derive(Debug, Error)]
enum HandleUpdateError {
    #[error("WebSocket error: {0}")]
    WebSocketError(axum::Error),
    #[error("NotesService error: {0}")]
    NotesServiceError(NotesServiceHandleError),
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "tag", content = "content")]
pub enum WebsocketMessage {
    #[serde(rename(serialize = "building"))]
    Building,
    #[serde(rename(serialize = "initialize"))]
    Initialize(Initialize),
    #[serde(rename(serialize = "update"))]
    Update(Vec<NoteData>),
    #[serde(rename(serialize = "remove"))]
    Remove(Vec<Uuid>),
}

async fn handle_updates_helper(
    notes_service: NotesServiceHandle,
    mut socket: WebSocket,
) -> Result<(), HandleUpdateError> {
    let build_finished = notes_service
        .get_build_finished()
        .await
        .map_err(HandleUpdateError::NotesServiceError)?;

    if !build_finished.has_occured() {
        let payload = WebsocketMessage::Building;
        let content = serde_json::to_string(&payload).unwrap();

        socket
            .send(Message::Text(content.into()))
            .await
            .map_err(HandleUpdateError::WebSocketError)?;

        build_finished.wait().await;
    }

    let (initialize, mut receiver) = notes_service
        .subscribe()
        .await
        .map_err(HandleUpdateError::NotesServiceError)?;

    {
        let payload = WebsocketMessage::Initialize(initialize);
        let content = serde_json::to_string(&payload).unwrap();

        socket
            .send(Message::Text(content.into()))
            .await
            .map_err(HandleUpdateError::WebSocketError)?;
    }

    loop {
        match receiver.recv().await {
            Ok(update) => {
                let payload = match update {
                    NoteUpdate::Update(updates) => WebsocketMessage::Update(updates),
                    NoteUpdate::Remove(removes) => WebsocketMessage::Remove(removes),
                };
                let content = serde_json::to_string(&payload).unwrap();

                socket
                    .send(Message::Text(content.into()))
                    .await
                    .map_err(HandleUpdateError::WebSocketError)?;
            }
            Err(broadcast::error::RecvError::Lagged(lag_error)) => {
                panic!("Lag error occurred {:?}", lag_error);
            }
            Err(broadcast::error::RecvError::Closed) => break Ok(()),
        }
    }
}

async fn handle_updates(
    State(notes_service): State<NotesServiceHandle>,
    websocket: ws::WebSocketUpgrade,
) -> impl IntoResponse {
    websocket.on_upgrade(async move |socket| {
        if let Err(error) = handle_updates_helper(notes_service, socket).await {
            println!("Error in websocket handler: {:?}", error);
        }
    })
}

pub fn router(actor: NotesServiceHandle) -> Router<()> {
    let cors = cors::CorsLayer::new()
        .allow_origin(cors::Any)
        .allow_methods([http::Method::GET, http::Method::POST])
        .allow_headers(cors::Any);

    Router::new()
        .route("/api/notes/{id}/content", get(get_note_content))
        .route("/api/updates", any(handle_updates))
        .with_state(actor)
        .layer(cors)
}
