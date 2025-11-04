use std::io;

use axum::{
    Json, Router,
    body::Body,
    extract::{Path, State, ws},
    response::{Html, IntoResponse},
    routing::{any, get},
};
use http::{Response, StatusCode};
use tower_http::cors;
use uuid::Uuid;

use crate::notes_service::{GetNoteMetadata, NotesServiceHandle, NotesServiceHandleError};

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

struct GetNoteMetadataResponse {
    result: Result<Option<GetNoteMetadata>, NotesServiceHandleError>,
}

impl IntoResponse for GetNoteMetadataResponse {
    fn into_response(self) -> Response<Body> {
        match self.result {
            Ok(Some(metadata)) => IntoResponse::into_response(Json(metadata)),
            Ok(None) => IntoResponse::into_response(StatusCode::NOT_FOUND),
            Err(_) => IntoResponse::into_response(StatusCode::INTERNAL_SERVER_ERROR),
        }
    }
}

async fn get_note_metadata(
    State(notes_service): State<NotesServiceHandle>,
    Path(id): Path<Uuid>,
) -> GetNoteMetadataResponse {
    let result = notes_service.get_note_metadata(id).await;

    GetNoteMetadataResponse { result }
}

async fn handle_updates(
    State(notes_service): State<NotesServiceHandle>,
    websocket: ws::WebSocketUpgrade,
) -> impl IntoResponse {
    websocket.on_upgrade(async |socket| {
        // let (sender, receiver) = socket.split();

        // TODO: Just have a pure server sent thing that sends everything. The
        // client connects once at the beginning, gets an initial graph, and
        // then receives all updates

        // Then the only other method is get content, which gets called when
        // content gets replaced (client finds out about this through websocket)
    })
}

pub fn router(actor: NotesServiceHandle) -> Router<()> {
    let cors = cors::CorsLayer::new()
        .allow_origin(cors::Any)
        .allow_methods([http::Method::GET, http::Method::POST])
        .allow_headers(cors::Any);

    Router::new()
        .route("/api/notes/{id}/content", get(get_note_content))
        .route("/api/notes/{id}/metadata", get(get_note_metadata))
        .route("/api/updates", any(handle_updates))
        .with_state(actor)
        .layer(cors)
}
