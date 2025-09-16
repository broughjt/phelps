use std::{
    collections::{HashMap, HashSet},
    fs, io,
    path::{Path, PathBuf},
    sync::{mpsc, Arc},
    time::Duration,
};

use bytes::Buf;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;
use thiserror::Error;
use typst::{
    diag::{PackageError, SourceDiagnostic, Warned},
    ecow::EcoVec,
    html::HtmlDocument,
    syntax::{FileId, VirtualPath},
};

use crate::{
    config::Config, package::{PackageService, PackageStorage}, system_world::{FileSlot, Resources, SystemWorld}
};

const POLL_INTERVAL: Duration = Duration::from_millis(300);

// TODO: We shouldn't hold this in memory, we should run build for the effect of producing an output in the build directory
pub struct BuildOutput {
    pub warnings: EcoVec<SourceDiagnostic>,
    pub document: HtmlDocument,
    pub raw_html: String,
}

#[derive(Debug, Error)]
pub enum BuildError {
    #[error("compilation error")]
    Compile(EcoVec<SourceDiagnostic>),
    #[error("export error")]
    Export(EcoVec<SourceDiagnostic>),
    #[error("write error")]
    Write(io::Error),
}

// pub struct BuildState {
//     pub result: Result<BuildOutput, BuildError>,
//     pub dependencies: HashSet<FileId>
// }

pub fn build<S>(
    resources: Arc<Resources>,
    package_storage: PackageStorage<S>,
    slots: Arc<Mutex<HashMap<FileId, FileSlot>>>,
    path: &Path,
    project_directory: &Path,
    build_subdirectory: &Path,
) -> Result<Warned<HashSet<FileId>>, BuildError>
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
    let document = result.map_err(BuildError::Compile)?;

    let output = typst_html::html(&document).map_err(BuildError::Export)?;

    let file_stem = main_id
        .vpath()
        .as_rootless_path()
        .file_stem()
        .ok_or_else(|| BuildError::Write(io::Error::other("Failed to get file extension")))?;
    let mut output_path = build_subdirectory.join(file_stem);
    output_path.set_extension("html");

    if let Err(error) = fs::create_dir(build_subdirectory)
        && error.kind() != io::ErrorKind::AlreadyExists
    {
        return Err(BuildError::Write(error));
    }
    fs::write(&output_path, &output).map_err(BuildError::Write)?;

    let dependencies = world.into_dependencies();
    let warned = Warned {
        output: dependencies,
        warnings,
    };

    Ok(warned)
}

pub fn watch<S>(
    resources: Arc<Resources>,
    package_storage: PackageStorage<S>,
    _slots: Arc<Mutex<HashMap<FileId, FileSlot>>>,
    paths: Vec<PathBuf>,
    config: &Config
) -> Result<Warned<HashSet<FileId>>, BuildError>
where
    S: Send + Sync + Clone,
    S: PackageService,
    PackageError: From<S::GetIndexServiceError>,
    PackageError: From<S::GetPackageServiceError>,
    S::GetPackageBuffer: Buf,
{
    let (sender, receiver) = mpsc::channel();

    let watch_config = notify::Config::default().with_poll_interval(POLL_INTERVAL);
    // TODO:
    let mut watcher = RecommendedWatcher::new(sender, watch_config).unwrap();

    // TODO:
    watcher.watch(&config.notes_subdirectory, RecursiveMode::Recursive).unwrap();

    for path in paths {
        let slots = Arc::new(Mutex::new(HashMap::new()));
        let _result = build(
            resources.clone(),
            package_storage.clone(),
            slots,
            &path,
            &config.project_directory,
            &config.build_subdirectory,
        );
    }

    loop {
        // TODO:
        let event = receiver.recv().unwrap().unwrap();

        if event.kind.is_create() || event.kind.is_modify() {
            if event.paths.len() == 1 {
                if event.paths[0].extension().is_some_and(|e| e == "typ") {
                    let slots = Arc::new(Mutex::new(HashMap::new()));
                    let _result = build(
                        resources.clone(),
                        package_storage.clone(),
                        slots,
                        &event.paths[0],
                        &config.project_directory,
                        &config.build_subdirectory
                    );
                }
            } else {
                unimplemented!()
            }
        }
    }
}
