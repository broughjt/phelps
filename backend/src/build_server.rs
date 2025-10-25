use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    str::FromStr,
    sync::Arc,
};

use bytes::Buf;
use ego_tree::{NodeId, NodeRef, Tree};
use http_body_util::Empty;
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::{
    client::legacy::{Client, connect::HttpConnector},
    rt::TokioExecutor,
};
use markup5ever::{LocalName, QualName, ns};
use notify::{Error, Event, EventHandler, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;
use petgraph::{prelude::DiGraphMap, visit::Bfs, Direction};
use scraper::{ElementRef, Html, Node, Selector};
use tokio::{runtime::Handle, sync::mpsc};
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
    project_directory: PathBuf,
    notes_subdirectory: PathBuf,
    build_subdirectory: PathBuf,
    package_storage: PackageStorage<
        HttpWrapper<ClientWrapper<HttpsConnector<HttpConnector>, Empty<hyper::body::Bytes>>>,
    >,
    resources: Arc<Resources>,
    slots: Arc<Mutex<HashMap<FileId, FileSlot>>>,
    is_source: HashSet<FileId>,
    receiver: mpsc::Receiver<Result<Event, Error>>,
    watcher: RecommendedWatcher,
    cancel: CancellationToken,
    graph: DiGraphMap<FileId, ()>,
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
            watcher,
            cancel,
            graph,
        })
    }

    pub fn startup(&mut self) {
        // TODO: Do async
        if self.build_subdirectory.exists() {
            // TODO:
            fs::remove_dir_all(&self.build_subdirectory).unwrap();
            // TODO:
            fs::create_dir(&self.build_subdirectory).unwrap();
        }

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

            self.handle_create(id).unwrap(); // TODO
        }

        self.watcher
            .watch(&self.project_directory, RecursiveMode::Recursive)
            .unwrap(); // TODO
    }

    pub async fn run(self) {
        // TODO: During this time, we are not listening for a cancel
        // signal. Maybe it takes a long time and then we ignore the user's
        // desire to end the program for a long time. That would suck. We should
        // fix that, probably by selecting against the cancel signal here too.
        let mut this = self;
        let mut this = tokio::task::spawn_blocking(move || {
            this.startup();

            this
        })
            .await
            .unwrap();

        loop {
            tokio::select! {
                option = this.receiver.recv() => if let Some(result) = option {
                    if let Ok(event) = result {
                        match event.kind {
                            EventKind::Access(_) | EventKind::Any | EventKind::Other => (),
                            EventKind::Create(_) => {
                                assert_eq!(event.paths.len(), 1);

                                let path = &event.paths[0];
                                let virtual_path = VirtualPath::within_root(path, &this.project_directory).unwrap();
                                let id = FileId::new(None, virtual_path);

                                if id.package().is_none()
                                    && path.extension().is_some_and(|e| e == "typ")
                                    && path.strip_prefix(&this.notes_subdirectory).is_ok() {
                                    this.handle_create(id).unwrap(); // TODO
                                }
                            },
                            EventKind::Modify(_) => {
                                assert_eq!(event.paths.len(), 1); // TODO

                                let path = &event.paths[0];
                                let virtual_path = VirtualPath::within_root(path, &this.project_directory).unwrap();
                                let id = FileId::new(None, virtual_path);

                                this.handle_modify(id).unwrap(); // TODO
                            },
                            EventKind::Remove(_) => {
                                assert_eq!(event.paths.len(), 1);

                                let path = &event.paths[0];
                                let virtual_path = VirtualPath::within_root(path, &this.project_directory).unwrap();
                                let id = FileId::new(None, virtual_path);

                                this.handle_remove(id);
                            },
                        }
                    }
                } else {
                    break
                },
                _ = this.cancel.cancelled() => {
                    println!("Build server actor cancelled");
                    this.receiver.close();

                    break
                }
            }
        }
    }

    fn handle_create(&mut self, i: FileId) -> Result<Warned<Vec<BuildOutput>>, EcoVec<SourceDiagnostic>> {
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

    fn handle_modify(&mut self, i: FileId) -> Vec<(FileId, Result<Warned<Vec<BuildOutput>>, EcoVec<SourceDiagnostic>>)> {
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

        // TODO: Wrong type, decide what it should be
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
                            .edges_directed(j, Direction::Incoming)
                            .map(|(k, _, _)| k)
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

    fn handle_remove(&mut self, id: FileId) {
        self.graph.remove_node(id);
        self.is_source.remove(&id);
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
    let mut stack: Vec<(NodeRef<T>, NodeId)> = Vec::new();

    {
        let root_id = tree.root_mut().id();
        stack.push((source, root_id));
    }

    while let Some((source_parent, destination_parent_id)) = stack.pop() {
        let mut destination_parent = tree.get_mut(destination_parent_id).unwrap();

        // Note: rev is efficient because children is a double-ended iterator
        for source_child in source_parent.children().rev() {
            let destination_child = destination_parent.append(source_child.value().clone());
            let destination_child_id = destination_child.id();

            stack.push((source_child, destination_child_id));
        }
    }

    tree
}

// fn find_links(html: &Html, document: &HtmlDocument)

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
    // TODO:
    let Warned {
        output: (html, document, dependencies),
        warnings,
    } = compile(resources, package_storage, slots, main_id).unwrap();

    let fragments = extract_note_fragments(&html, &document);
    let output = fragments
        .into_iter()
        .map(|(title, id, fragment)| {
             let links = Vec::new();

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

