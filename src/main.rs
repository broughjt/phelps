use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::sync::Arc;
use std::{fs ,io};
use std::path::PathBuf;

use clap::Parser;
use directories::ProjectDirs;
use http_body_util::Empty;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use parking_lot::Mutex;
use phelps::system_world::{FileSlot, Resources, SystemWorld};
use tokio::runtime::Runtime;
use typst::diag::Warned;
use typst::foundations::{Element, Selector};
use typst::introspection::MetadataElem;
use typst::syntax::FileId;
use typst::html::HtmlDocument;
use walkdir::{DirEntry, WalkDir};

use phelps::config::{Config, Cli, Commands};
use phelps::package::{ClientWrapper, HttpWrapper, PackageStorage};

pub const DEFAULT_REGISTRY: &str = "https://packages.typst.org";

pub const DEFAULT_NAMESPACE: &str = "preview";

fn main() -> Result<(), Box<dyn Error>> {
    let project_directories = ProjectDirs::from("", "", "phelps")
        .ok_or_else(|| io::Error::other("Couldn't determine standard directory locations"))?;

    let config_path: PathBuf = project_directories.config_dir().join("config.toml");
    let contents = fs::read_to_string(&config_path)?;
    let Config { root } = toml::from_str(&contents)?;
    let notes_directory = root.join("notes");
    let build_directory = root.join("build");

    fs::create_dir_all(&build_directory)?;

    let cli = Cli::try_parse()?;

    let runtime = Runtime::new()?;
    let https = HttpsConnectorBuilder::new()
        .with_native_roots()?
        .https_or_http()
        .enable_http1()
        .enable_http2()
        .build();
    // TODO: Why does `Client` take body as a struct-level generic and not as a
    // generic for `request`?
    let client: Client<_, Empty<hyper::body::Bytes>> = Client::builder(TokioExecutor::new())
        .build(https);
    let service = HttpWrapper(ClientWrapper(client));
    let package_storage = PackageStorage::new(
        project_directories.cache_dir().to_path_buf(),
        project_directories.data_dir().to_path_buf(),
        runtime.handle().clone(),
        service
    );

    let resources = Arc::new(Resources::new(root));

    let paths: HashSet<PathBuf> = WalkDir::new(&notes_directory)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|e| e == "typ"))
        .map(DirEntry::into_path)
        .collect();
    let slots: Arc<Mutex<HashMap<FileId, FileSlot>>> = Arc::new(Mutex::new(HashMap::new()));

    match cli.command {
        Commands::Watch => {
            unimplemented!();
        }
        Commands::Compile => {
            for path in paths {
                println!("{:?}", &path);

                let world = SystemWorld::new(
                    resources.clone(),
                    package_storage.clone(),
                    slots.clone(),
                    &path
                )?;
                let Warned { output: result, warnings: _warnings } = typst::compile::<HtmlDocument>(&world);

                match result {
                    Ok(document) => {
                        let selector = Selector::Elem(Element::of::<MetadataElem>(), None);
                        let metadatas = document.introspector.query(&selector);

                        println!("{:?}", metadatas);

                        match typst_html::html(&document) {
                            Ok(output) => {
                                let mut output_path = build_directory.clone();
                                output_path.push(path.file_stem().ok_or("Missing expected build file name")?);
                                output_path.set_extension("html");

                                fs::write(&output_path, &output)?;
                            }
                            Err(errors) => {
                                for error in errors {
                                    println!("{:?}", error);
                                }
                            }
                        }

                    }
                    Err(errors) => {
                        for error in errors {
                            println!("{:?}", error);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
