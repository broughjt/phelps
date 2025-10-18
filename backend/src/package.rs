use std::{fmt::Debug, fs, io, path::PathBuf, sync::Arc};

use bytes::{Buf, Bytes};
use http::{Method, StatusCode, Uri, uri::InvalidUri};
use http_body::Body;
use http_body_util::BodyExt;
use hyper::body::Incoming;
use hyper_util::client::legacy::{Client, connect::Connect};
use once_cell::sync::OnceCell;
use serde::Deserialize;
use tempfile::TempDir;
use thiserror::Error;
use tokio::runtime::Handle;
use tower_async::Service;
use typst::{
    diag::{PackageError, PackageResult},
    ecow::{EcoString, eco_format},
    syntax::package::{PackageSpec, PackageVersion},
};

pub const DEFAULT_REGISTRY: &str = "https://packages.typst.org";
pub const DEFAULT_NAMESPACE: &str = "preview";
pub const INDEX_URL: &str = "https://packages.typst.org/preview/index.json";

#[derive(Clone, Debug, Deserialize)]
pub struct Package {
    pub authors: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    pub description: String,
    #[serde(rename(deserialize = "entrypoint"))]
    pub entry_point: String,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    pub license: String,
    pub name: String,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(rename(deserialize = "updatedAt"))]
    pub updated_at: u64,
    pub version: String,
}

#[derive(Clone)]
pub struct HttpWrapper<S>(pub S);

pub struct GetIndexRequest;

impl From<GetIndexRequest> for http::Request<http_body_util::Empty<hyper::body::Bytes>> {
    fn from(_request: GetIndexRequest) -> Self {
        let (mut parts, body) = http::Request::default().into_parts();

        parts.method = Method::GET;
        parts.uri = Uri::from_static(INDEX_URL);

        http::Request::from_parts(parts, body)
    }
}

pub struct GetIndexResponse {
    pub packages: Vec<Package>,
}

#[derive(Debug, Error)]
pub enum GetIndexServiceError<E1, E2, E3> {
    #[error("underlying service error")]
    CallError(E1),
    #[error("error during body collection")]
    CollectError(E2),
    #[error("json decode error")]
    JsonError(E3),
    #[error("unexpected response")]
    UnexpectedResponse(http::response::Parts, Bytes),
}

impl<S, B> Service<GetIndexRequest> for HttpWrapper<S>
where
    S: Service<
            http::Request<http_body_util::Empty<hyper::body::Bytes>>,
            Response = http::Response<B>,
        >,
    B: Body,
{
    type Response = GetIndexResponse;
    type Error = GetIndexServiceError<S::Error, B::Error, serde_json::Error>;

    async fn call(&self, request: GetIndexRequest) -> Result<Self::Response, Self::Error> {
        let (parts, body) = self
            .0
            .call(request.into())
            .await
            .map_err(GetIndexServiceError::CallError)?
            .into_parts();
        if parts.status == StatusCode::OK {
            let buffer = body
                .collect()
                .await
                .map_err(GetIndexServiceError::CollectError)?
                .aggregate();
            let packages: Vec<Package> = serde_json::from_reader(buffer.reader())
                .map_err(GetIndexServiceError::JsonError)?;

            Ok(GetIndexResponse { packages })
        } else {
            let buffer = body
                .collect()
                .await
                .map_err(GetIndexServiceError::CollectError)?
                .to_bytes();

            Err(GetIndexServiceError::UnexpectedResponse(parts, buffer))
        }
    }
}

pub struct GetPackageRequest {
    specification: PackageSpec,
}

impl TryFrom<GetPackageRequest> for http::Request<http_body_util::Empty<hyper::body::Bytes>> {
    type Error = InvalidUri;

    fn try_from(
        GetPackageRequest { specification }: GetPackageRequest,
    ) -> Result<Self, Self::Error> {
        // TODO: Prolly change this
        // This is what typst-cli does right now
        // See https://github.com/typst/typst/blob/main/crates/typst-kit/src/package.rs#L175
        assert_eq!(specification.namespace, DEFAULT_NAMESPACE);

        let (mut parts, body) = http::Request::default().into_parts();
        let url = format!(
            "{DEFAULT_REGISTRY}/{DEFAULT_NAMESPACE}/{}-{}.tar.gz",
            specification.name, specification.version
        );

        parts.method = Method::GET;
        parts.uri = Uri::try_from(url)?;

        Ok(http::Request::from_parts(parts, body))
    }
}

pub struct GetPackageResponse<B> {
    pub buffer: B,
}

#[derive(Debug, Error)]
pub enum GetPackageError {
    #[error("package not found")]
    NotFound,
}

impl From<GetPackageError> for PackageError {
    fn from(_: GetPackageError) -> Self {
        // TODO: Fix this
        // I don't want to pass this through right now
        let fake = PackageSpec {
            namespace: EcoString::default(),
            name: EcoString::default(),
            version: PackageVersion {
                major: 0,
                minor: 0,
                patch: 0,
            },
        };

        Self::NotFound(fake)
    }
}

#[derive(Debug, Error)]
pub enum GetPackageServiceError<E1, E2> {
    #[error("invalid uri")]
    InvalidUri(InvalidUri),
    #[error("underlying service error")]
    CallError(E1),
    #[error("body collection error")]
    CollectError(E2),
    #[error("unexpected response")]
    UnexpectedResponse(http::response::Parts, Bytes),
}

impl From<GetPackageServiceError<hyper_util::client::legacy::Error, hyper::Error>>
    for PackageError
{
    fn from(
        error: GetPackageServiceError<hyper_util::client::legacy::Error, hyper::Error>,
    ) -> Self {
        PackageError::NetworkFailed(Some(eco_format!("{}", error)))
    }
}

impl<S, B> Service<GetPackageRequest> for HttpWrapper<S>
where
    S: Service<
            http::Request<http_body_util::Empty<hyper::body::Bytes>>,
            Response = http::Response<B>,
        >,
    // TODO: Is this right?
    B: Body + 'static,
{
    // TODO: Curse you http_body_util aggregate signature
    type Response = Result<GetPackageResponse<Box<dyn Buf + 'static>>, GetPackageError>;
    type Error = GetPackageServiceError<S::Error, B::Error>;

    async fn call(&self, request: GetPackageRequest) -> Result<Self::Response, Self::Error> {
        let request = request
            .try_into()
            .map_err(GetPackageServiceError::InvalidUri)?;
        let (parts, body) = self
            .0
            .call(request)
            .await
            .map_err(GetPackageServiceError::CallError)?
            .into_parts();

        if parts.status == StatusCode::OK {
            let buffer = body
                .collect()
                .await
                .map_err(GetPackageServiceError::CollectError)?
                .aggregate();

            Ok(Ok(GetPackageResponse {
                buffer: Box::new(buffer),
            }))
        } else if parts.status == StatusCode::NOT_FOUND {
            Ok(Err(GetPackageError::NotFound))
        } else {
            let buffer = body
                .collect()
                .await
                .map_err(GetPackageServiceError::CollectError)?
                .to_bytes();

            Err(GetPackageServiceError::UnexpectedResponse(parts, buffer))
        }
    }
}

pub trait PackageService {
    type GetIndexServiceError;

    fn get_index(&self) -> impl Future<Output = Result<Vec<Package>, Self::GetIndexServiceError>>;

    type GetPackageServiceError;
    type GetPackageBuffer: Buf;

    fn get_package(
        &self,
        specification: PackageSpec,
    ) -> impl Future<
        Output = Result<
            Result<Self::GetPackageBuffer, GetPackageError>,
            Self::GetPackageServiceError,
        >,
    >;
}

impl<S, B> PackageService for S
where
    S: Service<GetIndexRequest, Response = GetIndexResponse>,
    S: Service<GetPackageRequest, Response = Result<GetPackageResponse<B>, GetPackageError>>,
    B: Buf,
{
    type GetIndexServiceError = <S as Service<GetIndexRequest>>::Error;

    async fn get_index(&self) -> Result<Vec<Package>, Self::GetIndexServiceError> {
        self.call(GetIndexRequest).await.map(|r| r.packages)
    }

    type GetPackageServiceError = <S as Service<GetPackageRequest>>::Error;
    type GetPackageBuffer = B;

    async fn get_package(
        &self,
        specification: PackageSpec,
    ) -> Result<Result<Self::GetPackageBuffer, GetPackageError>, Self::GetPackageServiceError> {
        Ok(self
            .call(GetPackageRequest { specification })
            .await?
            .map(|r| r.buffer))
    }
}

#[derive(Clone)]
pub struct ClientWrapper<C, B>(pub Client<C, B>);

impl<C, B> Service<http::Request<B>> for ClientWrapper<C, B>
where
    C: Connect + Clone + Send + Sync + 'static,
    B: Body + Send + 'static + Unpin,
    B::Data: Send,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    type Response = http::Response<Incoming>;
    type Error = hyper_util::client::legacy::Error;

    fn call(
        &self,
        request: http::Request<B>,
    ) -> impl Future<Output = Result<Self::Response, Self::Error>> {
        self.0.request(request)
    }
}

impl From<GetIndexServiceError<hyper_util::client::legacy::Error, hyper::Error, serde_json::Error>>
    for PackageError
{
    fn from(
        error: GetIndexServiceError<
            hyper_util::client::legacy::Error,
            hyper::Error,
            serde_json::Error,
        >,
    ) -> Self {
        PackageError::NetworkFailed(Some(eco_format!("{error}")))
    }
}

#[derive(Clone, Debug)]
struct PackageStorageState {
    cache_directory: PathBuf,
    data_directory: PathBuf,
    index: OnceCell<Vec<Package>>,
}

#[derive(Clone)]
pub struct PackageStorage<S> {
    state: Arc<PackageStorageState>,
    handle: Handle,
    service: S,
}

impl<S> PackageStorage<S>
where
    S: PackageService,
    PackageError: From<S::GetIndexServiceError>,
    PackageError: From<S::GetPackageServiceError>,
    S::GetPackageBuffer: Buf,
{
    pub fn new(
        cache_directory: PathBuf,
        data_directory: PathBuf,
        handle: Handle,
        service: S,
    ) -> Self {
        Self {
            state: Arc::new(PackageStorageState {
                cache_directory,
                data_directory,
                index: OnceCell::new(),
            }),
            handle,
            service,
        }
    }

    pub fn get_index(&self) -> Result<&[Package], PackageError> {
        self.state
            .index
            .get_or_try_init(|| {
                self.handle
                    .block_on(self.service.get_index())
                    .map_err(Into::into)
            })
            .map(AsRef::as_ref)
    }

    fn download_package(&self, specification: &PackageSpec) -> PackageResult<()> {
        let data = self
            .handle
            .block_on(self.service.get_package(specification.clone()))??
            .reader();
        let package_directory = self.state.cache_directory.join(format!(
            "{}/{}/{}",
            specification.namespace, specification.name, specification.version
        ));
        let temporary_directory = TempDir::new().map_err(|_| {
            PackageError::Other(Some("Failed to create temporary_directory".into()))
        })?;
        let decompressed = flate2::read::GzDecoder::new(data);

        tar::Archive::new(decompressed)
            .unpack(&temporary_directory)
            .map_err(|error| PackageError::MalformedArchive(Some(eco_format!("{error}"))))?;

        fs::create_dir_all(&package_directory)
            .map_err(|e| PackageError::Other(Some(eco_format!("{}", e))))?;

        match fs::rename(&temporary_directory, &package_directory) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::DirectoryNotEmpty => Ok(()),
            Err(error) => Err(PackageError::Other(Some(eco_format!("{error}")))),
        }
    }

    pub fn prepare_package(&self, specification: &PackageSpec) -> PackageResult<PathBuf> {
        let subdirectory = format!(
            "{}/{}/{}",
            specification.namespace, specification.name, specification.version
        );

        let directory = self.state.data_directory.join(&subdirectory);
        if directory.exists() {
            return Ok(directory);
        }

        let directory = self.state.cache_directory.join(&subdirectory);
        if directory.exists() {
            return Ok(directory);
        }

        self.download_package(specification)?;
        if directory.exists() {
            return Ok(directory);
        }

        Err(PackageError::NotFound(specification.clone()))
    }
}
