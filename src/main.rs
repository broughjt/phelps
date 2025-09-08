use std::collections::HashSet;
use std::error::Error;
use std::{fs ,io};
use std::path::PathBuf;

use clap::Parser;
use directories::ProjectDirs;
use http_body_util::Empty;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use phelps::system_world::{Resources, SystemWorld};
use tokio::runtime::Runtime;
use typst_html::HtmlDocument;
use walkdir::{DirEntry, WalkDir};

use phelps::config::{Config, Cli, Commands};
use phelps::package::{ClientWrapper, HttpWrapper, PackageStorage};
// use phelps::file_system_resolver::FileSystemResolver;
// use phelps::world::{NoteWorld, Resources};

// const HTML_EXPORT_WARNING_MESSAGE: &str = "html export is under active development and incomplete";

pub const DEFAULT_REGISTRY: &str = "https://packages.typst.org";

pub const DEFAULT_NAMESPACE: &str = "preview";

fn main() -> Result<(), Box<dyn Error>> {
    let project_directories = ProjectDirs::from("", "", "phelps")
        .ok_or_else(|| io::Error::other("Couldn't determine standard directory locations"))?;

    let config_path: PathBuf = project_directories.config_dir().join("config.toml");
    let contents = fs::read_to_string(&config_path)?;
    let Config { root } = toml::from_str(&contents)?;
    let notes_directory = root.join("notes");

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
    let resources = Resources::new(root, package_storage);

    match cli.command {
        Commands::Watch => {
            unimplemented!();
        }
        Commands::Compile => {
            let paths: HashSet<PathBuf> = WalkDir::new(&notes_directory)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|e| e.path().extension().is_some_and(|e| e == "typ"))
                .map(DirEntry::into_path)
                .collect();

            for path in paths {
                let world = SystemWorld::with_path(&resources, &path)?;
                let Warned { output: result, warnings } = typst::compile::<HtmlDocument>(&world);
            }

            // let resolver = FileSystemResolver::new(notes_directory.clone());
            // let resources = Resources::new(resolver);

            // for path in entries {
            //     let world = NoteWorld::with_path(&resources, path.strip_prefix(&notes_directory)?);
            //     let Warned { output: result, warnings } = typst::compile::<HtmlDocument>(&world);

            //     if !warnings.is_empty() {
            //         let warnings = warnings.into_iter()
            //             .filter(|d| d.message != HTML_EXPORT_WARNING_MESSAGE);

            //         for warning in warnings {
            //             println!("{:?}", warning);
            //         }
            //     };

            //     let result = result.and_then(|output| typst_html::html(&output));

            //     match result {
            //         Ok(output) => {
            //             let output_path = notes_directory.join("build/output.html");
            //             // If the parent doesn't exist, this is a bug
            //             if !output_path.parent().unwrap().exists() {
            //                 fs::create_dir(output_path.parent().unwrap())?;
            //             }
            //             fs::write(&output_path, &output)?;
            //         }
            //         Err(errors) => {
            //             for error in errors {
            //                 println!("{:?}", error);
            //             }

            //             // TODO: Display error and stop compiling other files until fixed
            //             unimplemented!();
            //         }
            //     }
            // }
            // let client = Clie

            // let _path = notes_directory.join("test1.typ");

            // let runtime = Runtime::new()?;
            // let https = HttpsConnectorBuilder::new()
            //     .with_native_roots()?
            //     .https_or_http()
            //     .enable_http1()
            //     .enable_http2()
            //     .build();
            // let client: Client<_, Empty<hyper::body::Bytes>> = Client::builder(TokioExecutor::new())
            //     .build(https);
            // let mut service = HttpWrapper(ClientWrapper(client));

            // let specification = PackageSpec {
            //     namespace: "preview".into(),
            //     name: "fletcher".into(),
            //     version: PackageVersion { major: 0, minor: 5, patch: 8 }

            // };
            // let _package: Box<dyn Buf + 'static> = runtime.block_on(async {
            //     let package = service.get_package(specification).await?;

            //     Ok::<_, GetPackageServiceError<hyper_util::client::legacy::Error, hyper::Error>>(package)
            // })??;
        }
    }

    Ok(())
}
