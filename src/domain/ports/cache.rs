use crate::domain::entities::{ExecutionMetrics, ExecutionMetricsRecord};
use crate::domain::errors::CacheError;
use async_trait::async_trait;

#[async_trait]
pub trait TranslationCachePort: Send + Sync {
    async fn get(
        &self,
        source_lang: &str,
        target_lang: &str,
        provider: &str,
        text: &str,
    ) -> Result<Option<String>, CacheError>;

    async fn set(
        &self,
        source_lang: &str,
        target_lang: &str,
        provider: &str,
        original_text: &str,
        translated_text: &str,
    ) -> Result<(), CacheError>;

    async fn get_block(
        &self,
        source_lang: &str,
        target_lang: &str,
        provider: &str,
        block_text: &str,
    ) -> Result<Option<String>, CacheError>;

    async fn set_block(
        &self,
        source_lang: &str,
        target_lang: &str,
        provider: &str,
        original_block: &str,
        translated_block: &str,
    ) -> Result<(), CacheError>;

    async fn record_metrics(
        &self,
        provider: &str,
        target_lang: &str,
        metrics: &ExecutionMetrics,
    ) -> Result<(), CacheError> {
        let _ = (provider, target_lang, metrics);
        Ok(())
    }

    async fn get_metrics_history(
        &self,
        limit: usize,
    ) -> Result<Vec<ExecutionMetricsRecord>, CacheError> {
        let _ = limit;
        Ok(Vec::new())
    }
}

pub use TranslationCachePort as CachePort;
