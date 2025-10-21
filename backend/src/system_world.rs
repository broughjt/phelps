use std::{
    collections::{HashMap, HashSet},
    fs, mem,
    ops::DerefMut,
    path::{Path, PathBuf},
    sync::Arc,
};

use bytes::Buf;
use parking_lot::Mutex;
use thiserror::Error;
use time::{UtcDateTime, UtcOffset};
use typst::{
    Feature, Features, Library, World,
    diag::{FileError, FileResult, PackageError},
    foundations::{Bytes, Datetime},
    syntax::{FileId, Source},
    text::{Font, FontBook},
    utils::LazyHash,
};
use typst_kit::fonts::{FontSearcher, FontSlot};

use crate::package::{PackageService, PackageStorage};

#[derive(Debug)]
pub struct Resources {
    root: PathBuf,
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<FontSlot>,
}

impl Resources {
    pub fn new(root: PathBuf) -> Self {
        let fonts = FontSearcher::new().include_system_fonts(true).search();
        let library = Library::builder()
            .with_features(Features::from_iter([Feature::Html]))
            .build();

        Self {
            root,
            library: LazyHash::new(library),
            book: LazyHash::new(fonts.book),
            fonts: fonts.fonts,
        }
    }
}

struct State {
    main_id: FileId,
    time: UtcDateTime,
}

impl State {
    pub fn new(main_id: FileId, time: UtcDateTime) -> Self {
        Self { main_id, time }
    }
}

#[derive(Debug, Error)]
pub enum SystemWorldCreationError {
    #[error("path outside project root")]
    PathOutsideRoot,
}

pub struct SystemWorld<S> {
    resources: Arc<Resources>,
    package_storage: PackageStorage<S>,
    slots: Arc<Mutex<HashMap<FileId, FileSlot>>>,
    state: State,
    dependencies: Arc<Mutex<HashSet<FileId>>>,
}

impl<S> SystemWorld<S> {
    pub fn new(
        resources: Arc<Resources>,
        package_storage: PackageStorage<S>,
        slots: Arc<Mutex<HashMap<FileId, FileSlot>>>,
        main_id: FileId,
    ) -> Self {
        // let virtual_path = VirtualPath::within_root(path, &resources.root)
        //     .ok_or(SystemWorldCreationError::PathOutsideRoot)?;
        // let main_id = FileId::new(None, virtual_path);
        let state = State::new(main_id, UtcDateTime::now());
        let dependencies = Arc::new(Mutex::new(HashSet::new()));

        SystemWorld {
            resources,
            package_storage,
            slots,
            state,
            dependencies,
        }
    }

    pub fn dependencies(&self) -> Arc<Mutex<HashSet<FileId>>> {
        self.dependencies.clone()
    }

    pub fn into_dependencies(self) -> HashSet<FileId> {
        let mut guard = self.dependencies.lock();

        mem::take(guard.deref_mut())
    }
}

impl<S> World for SystemWorld<S>
where
    S: Send + Sync,
    S: PackageService,
    PackageError: From<S::GetIndexServiceError>,
    PackageError: From<S::GetPackageServiceError>,
    S::GetPackageBuffer: Buf,
{
    fn library(&self) -> &LazyHash<Library> {
        &self.resources.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.resources.book
    }

    fn main(&self) -> FileId {
        self.state.main_id
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id != self.state.main_id {
            self.dependencies.lock().insert(id);
        }

        let mut slots = self.slots.lock();
        let slot = slots.entry(id).or_insert_with(|| FileSlot::new(id));

        slot.source(&self.resources.root, id, &self.package_storage)
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id != self.state.main_id {
            self.dependencies.lock().insert(id);
        }

        let mut slots = self.slots.lock();
        let slot = slots.entry(id).or_insert_with(|| FileSlot::new(id));

        slot.file(&self.resources.root, id, &self.package_storage)
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.resources.fonts.get(index)?.get()
    }

    fn today(&self, offset: Option<i64>) -> Option<Datetime> {
        let offset = UtcOffset::from_hms(offset.unwrap_or(0).try_into().ok()?, 0, 0).ok()?;
        let time = self.state.time.checked_to_offset(offset)?;

        Some(Datetime::Date(time.date()))
    }
}

pub struct FileSlot {
    id: FileId,
    source: SlotCell<Source>,
    file: SlotCell<Bytes>,
}

impl FileSlot {
    pub fn new(id: FileId) -> Self {
        Self {
            id,
            file: SlotCell::new(),
            source: SlotCell::new(),
        }
    }

    pub fn accessed(&self) -> bool {
        self.source.accessed() || self.file.accessed()
    }

    pub fn reset(&mut self) {
        self.source.reset();
        self.file.reset();
    }

    pub fn source<S>(
        &mut self,
        root: &Path,
        file_id: FileId,
        package_storage: &PackageStorage<S>,
    ) -> FileResult<Source>
    where
        S: PackageService,
        PackageError: From<S::GetIndexServiceError>,
        PackageError: From<S::GetPackageServiceError>,
        S::GetPackageBuffer: Buf,
    {
        self.source.get_or_init(
            || read(root, file_id, package_storage),
            |data, previous| {
                let text = decode_utf8(&data)?;
                if let Some(mut previous) = previous {
                    previous.replace(text);

                    Ok(previous)
                } else {
                    Ok(Source::new(self.id, text.into()))
                }
            },
        )
    }

    pub fn file<S>(
        &mut self,
        root: &Path,
        file_id: FileId,
        package_storage: &PackageStorage<S>,
    ) -> FileResult<Bytes>
    where
        S: PackageService,
        PackageError: From<S::GetIndexServiceError>,
        PackageError: From<S::GetPackageServiceError>,
        S::GetPackageBuffer: Buf,
    {
        self.file.get_or_init(
            || read(root, file_id, package_storage),
            |data, _| Ok(Bytes::new(data)),
        )
    }
}

struct SlotCell<T> {
    data: Option<FileResult<T>>,
    fingerprint: u128,
    accessed: bool,
}

impl<T: Clone> SlotCell<T> {
    fn new() -> Self {
        Self {
            data: None,
            fingerprint: 0,
            accessed: false,
        }
    }

    fn accessed(&self) -> bool {
        self.accessed
    }

    fn reset(&mut self) {
        self.accessed = false;
    }

    // TODO: unused?
    // fn get(&self) -> Option<&FileResult<T>> {
    //     self.data.as_ref()
    // }

    fn get_or_init(
        &mut self,
        load: impl FnOnce() -> FileResult<Vec<u8>>,
        f: impl FnOnce(Vec<u8>, Option<T>) -> FileResult<T>,
    ) -> FileResult<T> {
        // If we accessed the file already in this compilation, retrieve it.
        if mem::replace(&mut self.accessed, true)
            && let Some(data) = &self.data
        {
            return data.clone();
        }

        // Read and hash the file.
        let result = load();
        let fingerprint = typst::utils::hash128(&result);

        // If the file contents didn't change, yield the old processed data.
        if mem::replace(&mut self.fingerprint, fingerprint) == fingerprint
            && let Some(data) = &self.data
        {
            return data.clone();
        }

        let previous = self.data.take().and_then(Result::ok);
        let value = result.and_then(|data| f(data, previous));
        self.data = Some(value.clone());

        value
    }
}

fn system_path<S>(
    root: &Path,
    id: FileId,
    package_storage: &PackageStorage<S>,
) -> FileResult<PathBuf>
where
    S: PackageService,
    PackageError: From<S::GetIndexServiceError>,
    PackageError: From<S::GetPackageServiceError>,
    S::GetPackageBuffer: Buf,
{
    let buffer: PathBuf;
    let mut root = root;
    if let Some(specification) = id.package() {
        buffer = package_storage.prepare_package(specification)?;

        root = &buffer;
    }

    id.vpath().resolve(root).ok_or(FileError::AccessDenied)
}

fn read<S>(root: &Path, id: FileId, package_storage: &PackageStorage<S>) -> FileResult<Vec<u8>>
where
    S: PackageService,
    PackageError: From<S::GetIndexServiceError>,
    PackageError: From<S::GetPackageServiceError>,
    S::GetPackageBuffer: Buf,
{
    let path = system_path(root, id, package_storage)?;
    let on_error = |e| FileError::from_io(e, &path);

    if fs::metadata(&path).map_err(on_error)?.is_dir() {
        Err(FileError::IsDirectory)
    } else {
        fs::read(&path).map_err(on_error)
    }
}

fn decode_utf8(buf: &[u8]) -> FileResult<&str> {
    // Remove UTF-8 BOM.
    Ok(std::str::from_utf8(
        buf.strip_prefix(b"\xef\xbb\xbf").unwrap_or(buf),
    )?)
}
