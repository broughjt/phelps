use axum::{
    Json, Router,
    body::Body,
    extract::{Path, State},
    response::{Html, IntoResponse},
    routing::get,
};
use http::{Response, StatusCode};
use tower_http::cors;
use uuid::Uuid;

use crate::service::{NoteMetadata, NotesActorHandle, NotesActorHandleError};

struct GetNoteContentResponse {
    result: Result<Result<Option<String>, ()>, NotesActorHandleError>,
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
    State(actor): State<NotesActorHandle>,
    Path(id): Path<Uuid>,
) -> GetNoteContentResponse {
    let result = actor.get_note_content(id).await;

    GetNoteContentResponse { result }
}

struct GetNoteMetadataResponse {
    result: Result<Option<NoteMetadata>, NotesActorHandleError>,
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
    State(actor): State<NotesActorHandle>,
    Path(id): Path<Uuid>,
) -> GetNoteMetadataResponse {
    let result = actor.get_note_metadata(id).await;

    GetNoteMetadataResponse { result }
}

pub fn router(actor: NotesActorHandle) -> Router<()> {
    let cors = cors::CorsLayer::new()
        .allow_origin(cors::Any)
        .allow_methods([http::Method::GET, http::Method::POST])
        .allow_headers(cors::Any);

    Router::new()
        .route("/api/notes/{id}/content", get(get_note_content))
        .route("/api/notes/{id}/metadata", get(get_note_metadata))
        .with_state(actor)
        .layer(cors)
}
