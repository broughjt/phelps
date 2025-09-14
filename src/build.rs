use std::{
    collections::{HashMap, HashSet},
    fs, io,
    path::Path,
    sync::Arc,
};

use bytes::Buf;
use parking_lot::Mutex;
use thiserror::Error;
use typst::{
    diag::{PackageError, SourceDiagnostic, Warned},
    ecow::EcoVec,
    html::HtmlDocument,
    syntax::{FileId, VirtualPath},
};

use crate::{
    package::{PackageService, PackageStorage},
    system_world::{FileSlot, Resources, SystemWorld},
};

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
