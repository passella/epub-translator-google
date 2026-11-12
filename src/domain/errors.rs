use std::fmt::{Display, Formatter, Result as FmtResult};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceLocation {
    pub file: &'static str,
    pub line: u32,
    pub column: u32,
}

impl SourceLocation {
    pub const fn new(file: &'static str, line: u32, column: u32) -> Self {
        Self { file, line, column }
    }
}

impl Display for SourceLocation {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}:{}:{}", self.file, self.line, self.column)
    }
}

#[macro_export]
macro_rules! here {
    () => {
        $crate::domain::errors::SourceLocation::new(file!(), line!(), column!())
    };
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessErrorDetails {
    pub command: String,
    pub args: Vec<String>,
    pub io_mode: String,
    pub exit_code: Option<i32>,
    pub os_error_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub message: String,
    pub location: SourceLocation,
}

impl Display for ProcessErrorDetails {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.message)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpErrorDetails {
    pub url: String,
    pub method: String,
    pub request_headers: Vec<(String, String)>,
    pub request_body: String,
    pub status_code: u16,
    pub response_headers: Vec<(String, String)>,
    pub response_body: String,
    pub message: String,
}

impl Display for HttpErrorDetails {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(
            f,
            "HTTP {} {} returned status {}: {}",
            self.method, self.url, self.status_code, self.message
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChapterTranslationError {
    pub chapter_index: usize,
    pub chapter_path: String,
    pub chapter_size: usize,
    pub chapter_preview_start: String,
    pub chapter_preview_end: String,
    pub location: SourceLocation,
    pub source: TranslationError,
}

impl Display for ChapterTranslationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(
            f,
            "Chapter translation error in '{}' at {}: {}",
            self.chapter_path, self.location, self.source
        )
    }
}

impl std::error::Error for ChapterTranslationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

#[allow(dead_code)]
#[derive(Error, Debug)]
pub enum EpubError {
    #[error("EPUB I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("ZIP format error: {0}")]
    Zip(String),

    #[error("XML parsing error: {0}")]
    Xml(String),

    #[error("Invalid EPUB structure: {0}")]
    InvalidStructure(String),

    #[error("Missing container.xml")]
    MissingContainerXml,

    #[error("Missing package document (.opf)")]
    MissingPackageDocument,

    #[error("EPUB verification failed: {0}")]
    VerificationFailed(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum TranslationError {
    #[error("HTTP request failed: {0}")]
    Http(String),

    #[error("HTTP request failed: {0}")]
    HttpDetailed(Box<HttpErrorDetails>),

    #[error("Serialization or parsing error: {0}")]
    Serialization(String),

    #[error("Rate limited by provider: {0}")]
    RateLimited(String),

    #[error("Execution process error: {0}")]
    Process(String),

    #[error("Process execution error: {0}")]
    ProcessDetailed(Box<ProcessErrorDetails>),

    #[error("Authentication error: {0}")]
    Authentication(String),

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Cache error: {0}")]
    Cache(String),

    #[error("XML validation error: {0}")]
    XmlValidation(String),
}

#[allow(dead_code)]
#[derive(Error, Debug)]
pub enum CacheError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

#[derive(Error, Debug)]
pub enum AppError {
    #[error(transparent)]
    Epub(#[from] EpubError),

    #[error(transparent)]
    Translation(#[from] TranslationError),

    #[error(transparent)]
    Cache(#[from] CacheError),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("User cancelled prompt")]
    UserCancelled,

    #[error(transparent)]
    ChapterTranslation(Box<ChapterTranslationError>),
}
