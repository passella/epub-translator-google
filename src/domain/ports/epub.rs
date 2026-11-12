use crate::domain::entities::{EpubBook, EpubVerificationReport};
use crate::domain::errors::EpubError;
use async_trait::async_trait;
use std::path::Path;

#[async_trait]
pub trait EpubPort: Send + Sync {
    async fn read_epub(&self, path: &Path) -> Result<EpubBook, EpubError>;
    async fn write_epub(&self, book: &EpubBook, path: &Path) -> Result<(), EpubError>;
    async fn verify_epub(&self, path: &Path) -> Result<EpubVerificationReport, EpubError>;
}
