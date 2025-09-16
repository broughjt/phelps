use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;

use axum::response::Html;
use axum::{Router, routing::get};
use clap::Parser;
use http_body_util::Empty;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use parking_lot::Mutex;
use tokio::net::TcpListener;
use tokio::runtime::Runtime;
use typst::syntax::FileId;
use walkdir::{DirEntry, WalkDir};

// use phelps::build::{build, watch};
use phelps::config::{Arguments, Commands, Config};
use phelps::package::{ClientWrapper, HttpWrapper, PackageStorage};
use phelps::system_world::{FileSlot, Resources};

const CONTENT: &str = "<h1>Hello there!</h1><p>General kenobi</p>";

fn main() -> Result<(), Box<dyn Error>> {
    let arguments = Arguments::try_parse()?;
    let config = Config::try_build()?;

    match arguments.command {
        Commands::Watch => watch(config),
        Commands::Compile => compile(config),
    }
}

fn watch(config: Config) -> Result<(), Box<dyn Error>> {
    let runtime = Runtime::new()?;

    runtime.block_on(async {
        let router = Router::new().route(
            "/api/note/67e55044-10b1-426f-9247-bb680e5fe0c8",
            get(|| async { Html(CONTENT) }),
        );
        let listener = TcpListener::bind("127.0.0.1:3000").await?;

        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown())
            .await
            .map_err(Into::into)
    })
}

fn compile(config: Config) -> Result<(), Box<dyn Error>> {
    todo!()
}

async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
}
