// use std::collections::HashMap;
use std::error::Error;
// use std::path::PathBuf;
// use std::sync::Arc;

use clap::Parser;
use phelps::build_server::BuildServer;
// use http_body_util::Empty;
// use hyper_rustls::HttpsConnectorBuilder;
// use hyper_util::client::legacy::Client;
// use hyper_util::rt::TokioExecutor;
// use parking_lot::Mutex;
use phelps::{router::router, service::NotesActorHandle};
use tokio::runtime::Runtime;
use tokio::{net::TcpListener, signal};
// use typst::syntax::FileId;
// use walkdir::{DirEntry, WalkDir};

// use phelps::build::{build, watch};
use phelps::config::{Arguments, Commands, Config};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
// use phelps::package::{ClientWrapper, HttpWrapper, PackageStorage};
// use phelps::system_world::{FileSlot, Resources};

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
        let cancel = CancellationToken::new();
        let tracker = TaskTracker::new();

        {
            let cancel = cancel.clone();

            tracker.spawn(async move {
                tokio::select! {
                    _ = signal::ctrl_c() => cancel.cancel(),
                    _ = cancel.cancelled() => ()
                }
            });
        }

        let build_server = BuildServer::try_build(
            config.project_directory,
            config.notes_subdirectory,
            config.build_subdirectory,
            config.cache_directory,
            config.data_directory,
            runtime.handle().clone(),
            cancel.clone(),
        )?;
        tracker.spawn(build_server.run());

        let (actor_handle, actor) = NotesActorHandle::build(cancel);
        tracker.spawn(actor.run());

        // TODO: Spawn a task for http server

        let router = router(actor_handle);
        let listener = TcpListener::bind("127.0.0.1:3000").await?;

        let http = axum::serve(listener, router)
            .with_graceful_shutdown(shutdown())
            .into_future();

        tracker.spawn(http);

        tracker.close();
        tracker.wait().await;

        Ok(())
    })
}

fn compile(_config: Config) -> Result<(), Box<dyn Error>> {
    todo!()
}

async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
}
