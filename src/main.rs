use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use http_body_util::Empty;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use parking_lot::Mutex;
use phelps::build::{build, watch};
use phelps::system_world::{FileSlot, Resources};
use tokio::runtime::Runtime;
use typst::syntax::FileId;
use walkdir::{DirEntry, WalkDir};

use phelps::config::{Arguments, Commands, Config};
use phelps::package::{ClientWrapper, HttpWrapper, PackageStorage};

pub const DEFAULT_REGISTRY: &str = "https://packages.typst.org";

pub const DEFAULT_NAMESPACE: &str = "preview";

fn main() -> Result<(), Box<dyn Error>> {
    let arguments = Arguments::try_parse()?;
    let config = Config::try_build()?;

    let runtime = Runtime::new()?;
    let https = HttpsConnectorBuilder::new()
        .with_native_roots()?
        .https_or_http()
        .enable_http1()
        .enable_http2()
        .build();
    // TODO: Why does `Client` take body as a struct-level generic and not as a
    // generic for `request`?
    let client: Client<_, Empty<hyper::body::Bytes>> =
        Client::builder(TokioExecutor::new()).build(https);
    let service = HttpWrapper(ClientWrapper(client));
    let package_storage = PackageStorage::new(
        config.cache_directory.clone(),
        config.data_directory.clone(),
        runtime.handle().clone(),
        service,
    );

    let resources = Arc::new(Resources::new(config.project_directory.clone()));

    let paths: Vec<PathBuf> = WalkDir::new(&config.notes_subdirectory)
        .into_iter()
        .map(|result| result.map(DirEntry::into_path))
        .filter(|result| result.as_ref().is_ok_and(|p| p.extension().is_some_and(|s| s == "typ")))
        // TODO:
        // .map(|result| {
        //     result.map_err(io::Error::from).and_then(|entry| {
        //         let virtual_path =
        //             VirtualPath::within_root(entry.path(), &config.notes_subdirectory)
        //                 .ok_or_else(|| io::Error::other("Couldn't resolve virtual path"))?;

        //         Ok(FileId::new(None, virtual_path))
        //     })
        // })
        .collect::<Result<_, _>>()?;
    let slots: Arc<Mutex<HashMap<FileId, FileSlot>>> = Arc::new(Mutex::new(HashMap::new()));
    // let state: HashMap<FileId, BuildOutput> = HashMap::new();
    // let dependents: HashMap<FileId, Vec<FileId>> = HashMap::new();

    match arguments.command {
        Commands::Watch => {
            watch(
                resources,
                package_storage,
                slots,
                paths,
                &config
            ).unwrap();
        }
        Commands::Compile => {
            for path in paths {
                let _output = build(
                    resources.clone(),
                    package_storage.clone(),
                    slots.clone(),
                    &path,
                    &config.project_directory,
                    &config.build_subdirectory
                )?;
            }
        }
    }

    Ok(())
}
