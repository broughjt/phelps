use std::error::Error;

use clap::Parser;
use phelps::build_service::BuildService;
use phelps::{http_service::router, notes_service::NotesServiceHandle};
use tokio::runtime::Runtime;
use tokio::{net::TcpListener, signal};

use phelps::config::{Arguments, Commands, Config};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

fn main() -> Result<(), Box<dyn Error>> {
    let arguments = Arguments::try_parse()?;
    let config = Config::try_build()?;

    match arguments.command {
        Commands::Watch => watch(config),
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

        let (notes_service_handle, notes_service) = NotesServiceHandle::build(
            cancel.clone(),
            config.build_subdirectory.clone(),
            config.default_note,
        );
        let build_service = BuildService::try_build(
            config.project_directory,
            config.notes_subdirectory,
            config.build_subdirectory,
            config.cache_directory,
            config.data_directory,
            runtime.handle().clone(),
            notes_service_handle.clone(),
            cancel.clone(),
        )?;

        tracker.spawn(build_service.run());
        tracker.spawn(notes_service.run());

        let router = router(notes_service_handle);
        let listener = TcpListener::bind("127.0.0.1:3000").await?;
        let http = axum::serve(listener, router)
            .with_graceful_shutdown(cancel.cancelled())
            .into_future();

        tracker.spawn(http);

        let listener = TcpListener::bind("127.0.0.1:3001").await?;
        let editor_service = todo!();
        let editor = EditorListener::new(listener, editor_service, cancel.cancelled_owned());

        tracker.spawn(editor);

        tracker.close();
        tracker.wait().await;

        Ok(())
    })
}
