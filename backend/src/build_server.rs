use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
    mem
};

use bytes::Buf;
use http_body_util::Empty;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::{client::legacy::Client, rt::TokioExecutor};
use notify::{Error, Event, EventHandler, RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;
use scraper::{ElementRef, Html, Node, Selector};
use tokio::{runtime::Handle, sync::mpsc};
use tokio_util::sync::CancellationToken;
use typst::{
    Document,
    diag::{PackageError, SourceDiagnostic, Warned},
    ecow::EcoVec,
    foundations::Element,
    html::HtmlDocument,
    model::HeadingElem,
    syntax::{FileId, VirtualPath},
};
use uuid::Uuid;
use walkdir::{DirEntry, WalkDir};

use crate::{
    package::{ClientWrapper, HttpWrapper, PackageService, PackageStorage},
    system_world::{FileSlot, Resources, SystemWorld},
};

pub struct MpscWrapper(pub mpsc::Sender<Result<Event, Error>>);

impl EventHandler for MpscWrapper {
    fn handle_event(&mut self, result: Result<Event, Error>) {
        let _ = self.0.blocking_send(result);
    }
}

pub struct BuildServer {
    receiver: mpsc::Receiver<Result<Event, Error>>,
    project_directory: PathBuf,
    build_subdirectory: PathBuf,
    watcher: RecommendedWatcher,
    cancel: CancellationToken,
}

impl BuildServer {
    pub fn try_build(
        project_directory: PathBuf,
        notes_subdirectory: PathBuf,
        build_subdirectory: PathBuf,
        cache_directory: PathBuf,
        data_directory: PathBuf,
        handle: Handle,
        cancel: CancellationToken,
    ) -> Result<Self, Error> {
        const BUFFER_SIZE: usize = 128;
        let (sender, receiver) = mpsc::channel(BUFFER_SIZE);

        let paths = WalkDir::new(&notes_subdirectory)
            .into_iter()
            .map(|result| result.map(DirEntry::into_path))
            .filter(|result| {
                result
                    .as_ref()
                    .is_ok_and(|path| path.extension().is_some_and(|s| s == "typ"))
            });
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
        let package_storage =
            PackageStorage::new(cache_directory, data_directory, handle.clone(), service);
        let resources = Arc::new(Resources::new(project_directory.clone()));

        for result in paths {
            // TODO
            let path = result.unwrap();
            println!("{:?}", path);
            let slots = Arc::new(Mutex::new(HashMap::new()));

            build(
                resources.clone(),
                package_storage.clone(),
                slots,
                &path,
                &project_directory,
            );
            // handle
            //     .spawn_blocking(|| {
            //         build(resources, package_storage, slots, path.clone(), project_directory.clone())
            //     })
            //     .await
            //     .unwrap();
        }
        todo!();

        let mut watcher = RecommendedWatcher::new(MpscWrapper(sender), Default::default())?;

        watcher.watch(&project_directory, RecursiveMode::Recursive)?;

        Ok(Self {
            receiver,
            project_directory,
            build_subdirectory,
            watcher,
            cancel,
        })
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                option = self.receiver.recv() => if let Some(event) = option {
                    println!("{:?}", event);
                } else {
                    break
                },
                _ = self.cancel.cancelled() => {
                    self.receiver.close();

                    break
                }
            }
        }
    }
}

fn compile<S>(
    resources: Arc<Resources>,
    package_storage: PackageStorage<S>,
    slots: Arc<Mutex<HashMap<FileId, FileSlot>>>,
    path: &Path,
    project_directory: &Path,
) -> Result<Warned<(Html, HtmlDocument)>, EcoVec<SourceDiagnostic>>
where
    S: Send + Sync,
    S: PackageService,
    PackageError: From<S::GetIndexServiceError>,
    PackageError: From<S::GetPackageServiceError>,
    S::GetPackageBuffer: Buf,
{
    let virtual_path = VirtualPath::within_root(path, project_directory).unwrap();
    let main_id = FileId::new(None, virtual_path);
    let world = SystemWorld::new(resources, package_storage, slots, main_id);

    let Warned {
        output: result,
        warnings,
    } = typst::compile::<HtmlDocument>(&world);
    let document = result?;

    let output = typst_html::html(&document)?;
    let html = Html::parse_document(&output);

    println!("{:?}", document);

    Ok(Warned {
        output: (html, document),
        warnings,
    })
}

pub struct NoteUuid(pub Uuid);

pub enum NoteUuidParseError {
    MissingPrefix,
    Uuid(uuid::Error),
}

impl FromStr for NoteUuid {
    type Err = NoteUuidParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.strip_prefix("note:")
            .ok_or(NoteUuidParseError::MissingPrefix)
            .and_then(|s| {
                Uuid::from_str(s)
                    .map(NoteUuid)
                    .map_err(NoteUuidParseError::Uuid)
            })
    }
}

// Very bad ugly bad bad code
fn extract_note_fragments(html: Html, document: &HtmlDocument) -> Vec<(Uuid, Html)> {
    let selector = typst::foundations::Selector::Elem(Element::of::<HeadingElem>(), None);
    let matches = document
        .introspector()
        .query(&selector)
        .into_iter()
        .filter_map(|c| {
            println!("{:?}", c);
            let uuid: NoteUuid = c.label().and_then(|l| {
                let foo = l.resolve();
                println!("{:?}", foo.as_str());

                foo.as_str().parse().ok()
            })?;

            Some((c.plain_text(), uuid.0))
        })
        .collect::<Vec<_>>();
    let matches_length = matches.len();

    let header_selector = Selector::parse("body > h2").unwrap();
    let headers = html.select(&header_selector);

    let fragments: Vec<_> = headers
        .map(|h| {
            let mut fragment = Html::new_fragment();
            // TODO:
            let text = h.text().next().unwrap();

            fragment.tree.root_mut().append(Node::Element(h.value().clone()));

            let siblings = h
                .next_siblings()
                .take_while(|&s| ElementRef::wrap(s).is_none_or(|e| e.value().name() != "h2"));

            for sibling in siblings {
                fragment.tree.root_mut().append(sibling.value().clone());
            }

            (text, fragment)
        })
        .collect();
    
    assert_eq!(matches_length, fragments.len());
    for (m, f) in matches.iter().zip(fragments.iter()) {
        assert_eq!(m.0, f.0);
    }

    matches.into_iter().zip(fragments).map(|((_, u), (_, f))| (u, f)).collect()
}

fn build<S>(
    resources: Arc<Resources>,
    package_storage: PackageStorage<S>,
    slots: Arc<Mutex<HashMap<FileId, FileSlot>>>,
    path: &Path,
    project_directory: &Path,
) where
    S: Send + Sync,
    S: PackageService,
    PackageError: From<S::GetIndexServiceError>,
    PackageError: From<S::GetPackageServiceError>,
    S::GetPackageBuffer: Buf,
{
    // TODO:
    let Warned {
        output: (html, document),
        warnings: _warnings,
    } = compile(resources, package_storage, slots, path, project_directory).unwrap();

    let fragments = extract_note_fragments(html, &document);

    for (uuid, f) in fragments {
        println!("{:?} {:?}", uuid, f);
    }
}
