use std::{
    collections::{HashMap, HashSet},
    error::Error,
    io,
    path::PathBuf,
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use bytes::Buf;
use ego_tree::{NodeRef, Tree};
use http_body_util::Empty;
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::{
    client::legacy::{Client, connect::HttpConnector},
    rt::TokioExecutor,
};
use markup5ever::{LocalName, QualName, ns};
use notify_debouncer_full::{
    DebounceEventHandler, DebounceEventResult, Debouncer, RecommendedCache, new_debouncer,
    notify::{self, RecommendedWatcher},
};
use parking_lot::Mutex;
use petgraph::{Direction, prelude::DiGraphMap, visit::Bfs};
use scraper::{ElementRef, Html, Node, Selector};
use tokio::{fs, runtime::Handle, sync::mpsc};
use tokio_util::sync::CancellationToken;
use typst::{
    Document,
    diag::{PackageError, SourceDiagnostic, Warned},
    ecow::EcoVec,
    model::HeadingElem,
    syntax::{FileId, VirtualPath},
};
use typst_html::HtmlDocument;
use uuid::Uuid;
use walkdir::{DirEntry, WalkDir};

use crate::{
    notes_service::{NoteData, NotesServiceHandle},
    package::{ClientWrapper, HttpWrapper, PackageService, PackageStorage},
    system_world::{FileSlot, Resources, SystemWorld},
};

pub struct MpscWrapper(pub mpsc::Sender<DebounceEventResult>);

impl DebounceEventHandler for MpscWrapper {
    fn handle_event(&mut self, result: DebounceEventResult) {
        let _ = self.0.blocking_send(result);
    }
}

pub struct BuildService {
    project_directory: PathBuf,
    notes_subdirectory: PathBuf,
    build_subdirectory: Arc<PathBuf>,
    package_storage: PackageStorage<
        HttpWrapper<ClientWrapper<HttpsConnector<HttpConnector>, Empty<hyper::body::Bytes>>>,
    >,
    resources: Arc<Resources>,
    slots: Arc<Mutex<HashMap<FileId, FileSlot>>>,
    is_source: HashSet<FileId>,
    notes_service: NotesServiceHandle,
    receiver: mpsc::Receiver<DebounceEventResult>,
    watcher: Debouncer<RecommendedWatcher, RecommendedCache>,
    cancel: CancellationToken,
    graph: DiGraphMap<FileId, ()>,
}

impl BuildService {
    // No it doesn't
    #[allow(clippy::too_many_arguments)]
    pub fn try_build(
        project_directory: PathBuf,
        notes_subdirectory: PathBuf,
        build_subdirectory: PathBuf,
        cache_directory: PathBuf,
        data_directory: PathBuf,
        handle: Handle,
        notes_service: NotesServiceHandle,
        cancel: CancellationToken,
    ) -> Result<Self, notify::Error> {
        const BUFFER_SIZE: usize = 128;
        const DEBOUNCE_TIMEOUT: Duration = Duration::from_millis(500);

        let (sender, receiver) = mpsc::channel(BUFFER_SIZE);

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
        let slots = Arc::new(Mutex::new(HashMap::new()));

        let graph = DiGraphMap::new();
        let is_source = HashSet::new();

        // let watcher = RecommendedWatcher::new(MpscWrapper(sender), Default::default())?;
        let watcher = new_debouncer(DEBOUNCE_TIMEOUT, None, MpscWrapper(sender))?;

        Ok(Self {
            receiver,
            project_directory,
            notes_subdirectory,
            build_subdirectory: Arc::new(build_subdirectory),
            package_storage,
            resources,
            slots,
            is_source,
            notes_service,
            watcher,
            cancel,
            graph,
        })
    }

    pub async fn start(&mut self) -> Result<(), Box<dyn Error>> {
        if self.build_subdirectory.exists() {
            fs::remove_dir_all(self.build_subdirectory.as_ref()).await?;
            fs::create_dir(self.build_subdirectory.as_ref()).await?;
        }

        let walker = WalkDir::new(&self.notes_subdirectory);
        let paths = tokio::task::spawn_blocking(|| {
            walker.into_iter().filter_map(|result| {
                result
                    .map(DirEntry::into_path)
                    .ok()
                    .filter(|path| path.extension().is_some_and(|s| s == "typ"))
            })
        })
        .await
        .unwrap();

        for path in paths {
            let virtual_path = VirtualPath::within_root(&path, &self.project_directory).unwrap();
            let id = FileId::new(None, virtual_path);

            let _ = self.handle_create(id).await;
        }

        let _ = self.notes_service.set_build_finished().await;

        self.watcher
            .watch(&self.project_directory, notify::RecursiveMode::Recursive)?;

        Ok(())
    }

    pub async fn run(mut self) {
        // I just don't care
        let cancel = self.cancel.clone();

        tokio::select! {
            _ = self.start() => (),
            _ = cancel.cancelled() => {
                println!("Build server cancelled");
                self.receiver.close();

                return
            }
        };

        loop {
            tokio::select! {
                option = self.receiver.recv() => if let Some(result) = option {
                    if let Ok(events) = result {

                        for event in events {
                            use notify::EventKind;

                            match event.kind {
                                EventKind::Access(_) | EventKind::Any | EventKind::Other => (),
                                EventKind::Create(_) => {
                                    assert_eq!(event.paths.len(), 1);

                                    let path = &event.paths[0];
                                    let virtual_path = VirtualPath::within_root(path, &self.project_directory).unwrap();
                                    let id = FileId::new(None, virtual_path);

                                    if id.package().is_none()
                                        && path.extension().is_some_and(|e| e == "typ")
                                        && path.strip_prefix(&self.notes_subdirectory).is_ok() {
                                        self.handle_create(id).await;
                                    }
                                },
                                EventKind::Modify(_) => {
                                    assert_eq!(event.paths.len(), 1); // TODO

                                    let path = &event.paths[0];
                                    let virtual_path = VirtualPath::within_root(path, &self.project_directory).unwrap();
                                    let id = FileId::new(None, virtual_path);
                                    let is_source = id.package().is_none()
                                        && path.extension().is_some_and(|e| e == "typ")
                                        && path.strip_prefix(&self.notes_subdirectory).is_ok();

                                    if self.graph.contains_node(id) || is_source {
                                        self.handle_modify(id).await;
                                    }
                                },
                                EventKind::Remove(_) => {
                                    assert_eq!(event.paths.len(), 1);

                                    let path = &event.paths[0];
                                    let virtual_path = VirtualPath::within_root(path, &self.project_directory).unwrap();
                                    let id = FileId::new(None, virtual_path);

                                    self.handle_remove(id).await;
                                },
                            }
                        }
                    }
                } else {
                    break
                },
                _ = cancel.cancelled() => {
                    println!("Build server cancelled");
                    self.receiver.close();

                    break
                }
            }
        }
    }

    async fn handle_create(&mut self, i: FileId) {
        match build(
            self.resources.clone(),
            self.package_storage.clone(),
            self.slots.clone(),
            self.build_subdirectory.clone(),
            i,
        )
        .await
        {
            Ok(Ok((warned, dependencies))) => {
                self.graph.add_node(i);
                for j in dependencies {
                    self.graph.add_edge(i, j, ());
                }

                let _ = self.notes_service.create_notes(i, Ok(warned)).await;
            }
            Ok(Err(errors)) => {
                let _ = self.notes_service.create_notes(i, Err(errors)).await;
            }
            Err(error) => {
                // Here we failed to write on of the fragments to the build
                // directory. This should result in a fatal error, so we need to
                // tell the rest of the application to shutdown.

                println!("Failed to write fragment to build directory: {}", error);
                self.cancel.cancel();
            }
        }
    }

    async fn handle_modify(&mut self, i: FileId) {
        // TODO: Next we need to debug creates and updates until we get the
        // behavior we're expecting all the way through. Then we can work on the
        // UI in earnest.
        let mut bfs = Bfs::new(&self.graph, i);
        let mut dependents = Vec::new();

        {
            dependents.push(i);

            let mut slots = self.slots.lock();

            // Note: BFS starts by traversing i, so we don't need to do that manually
            while let Some(j) = bfs.next(&self.graph) {
                if self.is_source.contains(&j) {
                    dependents.push(j);
                }
                slots.get_mut(&j).unwrap().reset();
            }
        }

        let mut results = Vec::with_capacity(dependents.len());

        for j in dependents {
            let result = build(
                self.resources.clone(),
                self.package_storage.clone(),
                self.slots.clone(),
                self.build_subdirectory.clone(),
                j,
            )
            .await;

            match result {
                Ok(Ok((warned, dependencies))) => {
                    let ks: Vec<FileId> = self
                        .graph
                        .neighbors_directed(j, Direction::Incoming)
                        .collect();

                    for k in ks {
                        self.graph.remove_edge(k, j);
                    }
                    for k in dependencies {
                        if k.package().is_none() {
                            self.graph.add_edge(k, j, ());
                        }
                    }

                    results.push((j, Ok(warned)));
                }
                Ok(Err(error)) => results.push((j, Err(error))),
                Err(error) => {
                    // We failed to save the fragment to the build directory, we
                    // need to tell the rest of application to shutdown

                    println!("Failed to save fragment to build directory: {}", error);
                    self.cancel.cancel();
                }
            }
        }

        let _ = self.notes_service.update_notes(results).await;
    }

    async fn handle_remove(&mut self, i: FileId) {
        self.graph.remove_node(i);
        self.is_source.remove(&i);

        // Note, notes service handles clean up of fragment files in build
        // directory.
        let _ = self.notes_service.remove_notes(i).await;
    }
}

type CompileOutput = (Html, HtmlDocument, HashSet<FileId>);

fn compile<S>(
    resources: Arc<Resources>,
    package_storage: PackageStorage<S>,
    slots: Arc<Mutex<HashMap<FileId, FileSlot>>>,
    main_id: FileId,
) -> Result<Warned<CompileOutput>, EcoVec<SourceDiagnostic>>
where
    S: Send + Sync,
    S: PackageService,
    PackageError: From<S::GetIndexServiceError>,
    PackageError: From<S::GetPackageServiceError>,
    S::GetPackageBuffer: Buf,
{
    let world = SystemWorld::new(resources, package_storage, slots, main_id);

    let Warned {
        output: result,
        warnings,
    } = typst::compile::<HtmlDocument>(&world);
    let document = result?;

    let output = typst_html::html(&document)?;
    let html = Html::parse_document(&output);

    Ok(Warned {
        output: (html, document, world.into_dependencies()),
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

pub struct NoteLink(pub Uuid);

pub enum NoteLinkParseError {
    MissingPrefix,
    Uuid(uuid::Error),
}

impl FromStr for NoteLink {
    type Err = NoteLinkParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.strip_prefix("note://")
            .ok_or(NoteLinkParseError::MissingPrefix)
            .and_then(|s| {
                Uuid::from_str(s)
                    .map(NoteLink)
                    .map_err(NoteLinkParseError::Uuid)
            })
    }
}

fn extract_note_fragments(html: &Html, document: &HtmlDocument) -> Vec<(String, Uuid, Html)> {
    let selector =
        typst::foundations::Selector::Elem(typst::foundations::Element::of::<HeadingElem>(), None);
    let matches: HashMap<Uuid, String> = document
        .introspector()
        .query(&selector)
        .into_iter()
        .filter_map(|c| {
            let NoteUuid(uuid) = c.label().and_then(|l| {
                let t = l.resolve();

                t.as_str().parse().ok()
            })?;
            let text = c.plain_text();
            let stripped = text.strip_prefix("Section").unwrap_or(&text);

            Some((uuid, stripped.into()))
        })
        .collect();

    let header_selector = Selector::parse("body > h2").unwrap();
    let headers = html.select(&header_selector);

    let mut fragments: HashMap<String, Html> = headers
        .map(|h| {
            let mut fragment = Html::new_fragment();
            let article_name = QualName::new(None, ns!(html), LocalName::from("article"));
            let article_element = scraper::node::Element::new(article_name, Vec::new());
            let mut root = fragment.tree.root_mut();
            let mut article = root.append(Node::Element(article_element));
            let text = h.text().next().unwrap_or_default();

            let siblings = h
                .next_siblings()
                .take_while(|&s| ElementRef::wrap(s).is_none_or(|e| e.value().name() != "h2"));

            for sibling in siblings {
                article.append_subtree(clone_subtree(sibling));
            }

            (text.into(), fragment)
        })
        .collect();

    // Note: This check is not sufficient. We should check injectivity of both
    // partial maps or something, I don't know. Point being, terrible things
    // might happen but I just don't really care unless this bites me later
    assert_eq!(matches.len(), fragments.len());

    matches
        .into_iter()
        .filter_map(|(uuid, title)| {
            fragments
                .remove(&title)
                .map(|fragment| (title, uuid, fragment))
        })
        .collect()
}

fn clone_subtree<T: Clone>(source: NodeRef<T>) -> Tree<T> {
    let mut tree = Tree::new(source.value().clone());
    let mut queue = std::collections::VecDeque::new();
    queue.push_back((source, tree.root().id()));

    while let Some((source_node, destination_id)) = queue.pop_front() {
        let mut destination_node = tree.get_mut(destination_id).unwrap();

        for source_child in source_node.children() {
            let destination_child_id = destination_node.append(source_child.value().clone()).id();
            queue.push_back((source_child, destination_child_id));
        }
    }

    tree
}

fn find_links(html: &Html) -> Vec<Uuid> {
    let selector = Selector::parse("a").unwrap();

    html.select(&selector)
        .filter_map(|element| {
            element.attr("href").and_then(|href| {
                if let Ok(NoteLink(uuid)) = href.parse() {
                    Some(uuid)
                } else {
                    None
                }
            })
        })
        .collect()
}

type BuildOutputs = (Warned<Vec<NoteData>>, HashSet<FileId>);

async fn build<S>(
    resources: Arc<Resources>,
    package_storage: PackageStorage<S>,
    slots: Arc<Mutex<HashMap<FileId, FileSlot>>>,
    build_subdirectory: Arc<PathBuf>,
    main_id: FileId,
) -> Result<Result<BuildOutputs, EcoVec<SourceDiagnostic>>, io::Error>
where
    S: Send + Sync + 'static,
    S: PackageService,
    PackageError: From<S::GetIndexServiceError>,
    PackageError: From<S::GetPackageServiceError>,
    S::GetPackageBuffer: Buf,
{
    let result = tokio::task::spawn_blocking(move || {
        let Warned {
            output: (html, document, dependencies),
            warnings,
        } = compile(resources, package_storage, slots, main_id)?;
        let fragments = extract_note_fragments(&html, &document);
        let (outputs, writes) = fragments
            .into_iter()
            .map(|(title, id, fragment)| {
                let links = find_links(&fragment);
                let output = NoteData { title, id, links };

                let content = fragment.html();
                let path = build_subdirectory.join(format!("{}.html", id));
                let write = fs::write(path, content);

                (output, write)
            })
            .unzip::<_, _, Vec<_>, Vec<_>>();

        Ok((
            Warned {
                output: (outputs, writes),
                warnings,
            },
            dependencies,
        ))
    })
    .await
    // If code in this task panics, we should panic
    .unwrap();

    match result {
        Ok((
            Warned {
                output: (outputs, writes),
                warnings,
            },
            dependencies,
        )) => {
            futures::future::join_all(writes)
                .await
                .into_iter()
                // `try_for_each` assumes you're calling an effect. For us, we
                // just want to check if all writes succeeded.
                .try_for_each(|result| result)?;

            Ok(Ok((
                Warned {
                    output: outputs,
                    warnings,
                },
                dependencies,
            )))
        }
        Err(errors) => Ok(Err(errors)),
    }
}
