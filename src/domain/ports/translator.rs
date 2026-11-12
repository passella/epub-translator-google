use crate::domain::entities::{Language, TranslationResult};
use crate::domain::errors::TranslationError;
use async_trait::async_trait;

#[async_trait]
pub trait TranslatorPort: Send + Sync {
    async fn translate_html(
        &self,
        html: &str,
        source_lang: &Language,
        target_lang: &Language,
    ) -> Result<TranslationResult, TranslationError>;

    async fn translate_batch(
        &self,
        texts: &[String],
        source_lang: &Language,
        target_lang: &Language,
    ) -> Result<Vec<TranslationResult>, TranslationError>;

    fn provider_name(&self) -> &'static str;

    fn model_name(&self) -> Option<&str> {
        None
    }
}
