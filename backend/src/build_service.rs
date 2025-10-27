use std::{
    collections::{HashMap, HashSet}, error::Error, path::PathBuf, str::FromStr, sync::Arc
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
use notify::{Event, EventHandler, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;
use petgraph::{prelude::DiGraphMap, visit::Bfs, Direction};
use scraper::{ElementRef, Html, Node, Selector};
use tokio::{fs::{self, File}, runtime::Handle, sync::mpsc, io::AsyncWriteExt};
use tokio_util::sync::CancellationToken;
use typst::{
    Document,
    diag::{PackageError, SourceDiagnostic, Warned},
    ecow::EcoVec,
    html::HtmlDocument,
    model::HeadingElem,
    syntax::{FileId, VirtualPath},
};
use uuid::Uuid;
use walkdir::{DirEntry, WalkDir};

use crate::{
    notes_service::{CreateNoteMetadata, NotesServiceHandle}, package::{ClientWrapper, HttpWrapper, PackageService, PackageStorage}, system_world::{FileSlot, Resources, SystemWorld}
};

pub struct MpscWrapper(pub mpsc::Sender<Result<Event, notify::Error>>);

impl EventHandler for MpscWrapper {
    fn handle_event(&mut self, result: Result<Event, notify::Error>) {
        let _ = self.0.blocking_send(result);
    }
}

pub struct BuildService {
    project_directory: PathBuf,
    notes_subdirectory: PathBuf,
    build_subdirectory: PathBuf,
    package_storage: PackageStorage<
        HttpWrapper<ClientWrapper<HttpsConnector<HttpConnector>, Empty<hyper::body::Bytes>>>,
    >,
    resources: Arc<Resources>,
    slots: Arc<Mutex<HashMap<FileId, FileSlot>>>,
    is_source: HashSet<FileId>,
    notes_service: NotesServiceHandle,
    receiver: mpsc::Receiver<Result<Event, notify::Error>>,
    watcher: RecommendedWatcher,
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

        // TODO:
        let watcher = RecommendedWatcher::new(MpscWrapper(sender), Default::default())?;

        Ok(Self {
            receiver,
            project_directory,
            notes_subdirectory,
            build_subdirectory,
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
            fs::remove_dir_all(&self.build_subdirectory).await?;
            fs::create_dir(&self.build_subdirectory).await?;
        }
            
        // TODO: This probably does lots of blocking operations
        let paths = WalkDir::new(&self.notes_subdirectory)
            .into_iter()
            .filter_map(|result| {
                result
                    .map(DirEntry::into_path)
                    .ok()
                    .filter(|path| path.extension().is_some_and(|s| s == "typ"))
            });
        
        for path in paths {
            let virtual_path = VirtualPath::within_root(&path, &self.project_directory).unwrap();
            let id = FileId::new(None, virtual_path);
            
            let _ = self.handle_create(id).await;
        }
            
        self.watcher.watch(&self.project_directory, RecursiveMode::Recursive)?;

        Ok(())
    }

    pub async fn run(mut self) {
        // I just don't care
        let cancel = self.cancel.clone();

        tokio::select! {
            _ = self.start() => (),
            _ = cancel.cancelled() => {
                println!("Build server actor cancelled");
                self.receiver.close();

                return
            }
        };

        loop {
            tokio::select! {
                option = self.receiver.recv() => if let Some(result) = option {
                    if let Ok(event) = result {
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
                } else {
                    break
                },
                _ = cancel.cancelled() => {
                    println!("Build server actor cancelled");
                    self.receiver.close();

                    break
                }
            }
        }
    }

    fn create(&mut self, i: FileId) -> Result<Warned<Vec<BuildOutput>>, EcoVec<SourceDiagnostic>> {
        println!("create {:?}", i);
        let (Warned { output: outputs, warnings }, dependencies) = build(
            self.resources.clone(),
            self.package_storage.clone(),
            self.slots.clone(),
            i,
        )?;

        self.graph.add_node(i);
        for j in dependencies {
            if j.package().is_none() {
                self.graph.add_edge(j, i, ());
            }
        }

        self.is_source.insert(i);

        Ok(Warned { output: outputs, warnings })
    }

    async fn handle_create(&mut self, i: FileId) {
        match self.create(i) {
            Ok(Warned { output: outputs, warnings }) => {
                for output in &outputs {
                    let path = self.build_subdirectory.join(format!("{}.html", output.id));
                    if let Ok(mut file) = File::create(path).await {
                        let _ = file.write_all(output.fragment.as_bytes()).await;
                    }
                }

                let outputs = outputs
                    .into_iter()
                    .map(|BuildOutput { title, id, links, .. }| {
                        CreateNoteMetadata { title, id, links }
                    })
                    .collect();

                let _ = self.notes_service.create_notes(i, Ok(Warned { output: outputs, warnings })).await;
            }
            Err(error) => {
                let _ = self.notes_service.create_notes(i, Err(error)).await;
            }
        }
    }

    #[allow(clippy::type_complexity)]
    fn modify(&mut self, i: FileId) -> Vec<(FileId, Result<Warned<Vec<BuildOutput>>, EcoVec<SourceDiagnostic>>)> {
        println!("modify {:?}", i);
        let mut bfs = Bfs::new(&self.graph, i);
        let mut dependents = Vec::new();

        {
            let mut slots = self.slots.lock();
            
            // Note: BFS starts by traversing i, so we don't need to do that manually
            while let Some(j) = bfs.next(&self.graph) {
                if self.is_source.contains(&j) {
                    dependents.push(j);
                }
                slots.get_mut(&j).unwrap().reset();
            }
        }

        dependents
            .iter()
            .map(|&j| {
                let result = build(
                    self.resources.clone(),
                    self.package_storage.clone(),
                    self.slots.clone(),
                    j
                )
                    .map(|(warned, dependencies)| {
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

                        warned
                    });

                (j, result)
            })
            .collect()
    }

    async fn handle_modify(&mut self, id: FileId) {
        // I am pretty ashamed of this code. Honestly I'm ashamed of all the
        // code in the entire project but having to write this handler brought
        // that shame to a point. @Conman: "It works"

        let results = self.modify(id);
        let mut results_new = Vec::with_capacity(results.len());

        for (file_id, result) in results {
            match result {
                Ok(Warned { output: outputs, warnings }) => {
                    for output in &outputs {
                        let path = self.build_subdirectory.join(format!("{}.html", output.id));
                        if let Ok(mut file) = File::create(path).await {
                            let _ = file.write_all(output.fragment.as_bytes()).await;
                        }
                    }

                    let outputs = outputs
                        .into_iter()
                        .map(|BuildOutput { title, id, links, .. }| {
                            CreateNoteMetadata { title, id, links }
                        })
                        .collect();

                    results_new.push((file_id, Ok(Warned { output: outputs, warnings })));
                }
                Err(error) => {
                    results_new.push((file_id, Err(error)));
                }
            }
        }

        let _ = self.notes_service.update_notes(results_new).await;
    }

    fn remove(&mut self, file_id: FileId) {
        self.graph.remove_node(file_id);
        self.is_source.remove(&file_id);
    }

    async fn handle_remove(&mut self, file_id: FileId) {
        self.remove(file_id);
        let _ = self.notes_service.remove_notes(file_id).await;
    }
}

type CompilationOutput = (Html, HtmlDocument, HashSet<FileId>);

fn compile<S>(
    resources: Arc<Resources>,
    package_storage: PackageStorage<S>,
    slots: Arc<Mutex<HashMap<FileId, FileSlot>>>,
    main_id: FileId,
) -> Result<
        Warned<CompilationOutput>,
        EcoVec<SourceDiagnostic>
     >
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
    println!("{}", output);
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
                article.append_subtree(copy_subtree(sibling));
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
        .filter_map(|(uuid, title)| fragments.remove(&title).map(|fragment| (title, uuid, fragment)))
        .collect()
}

fn copy_subtree<T: Clone>(source: NodeRef<T>) -> Tree<T> {
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

    html
        .select(&selector)
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

pub struct BuildOutput {
    pub title: String,
    pub id: Uuid,
    pub fragment: String,
    pub links: Vec<Uuid>
}

type BuildOutputs = (Warned<Vec<BuildOutput>>, HashSet<FileId>);

fn build<S>(
    resources: Arc<Resources>,
    package_storage: PackageStorage<S>,
    slots: Arc<Mutex<HashMap<FileId, FileSlot>>>,
    main_id: FileId
) -> Result<
        BuildOutputs,
        EcoVec<SourceDiagnostic>
     >
where
    S: Send + Sync,
    S: PackageService,
    PackageError: From<S::GetIndexServiceError>,
    PackageError: From<S::GetPackageServiceError>,
    S::GetPackageBuffer: Buf,
{
    let Warned {
        output: (html, document, dependencies),
        warnings,
    } = compile(resources, package_storage, slots, main_id)?;

    let fragments = extract_note_fragments(&html, &document);
    let output = fragments
        .into_iter()
        .map(|(title, id, fragment)| {
             let links = find_links(&fragment);

             BuildOutput {
                 title,
                 id,
                 fragment: fragment.html(),
                 links
             }
        })
        .collect();

    Ok((Warned { output, warnings }, dependencies))
}

