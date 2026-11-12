use crate::domain::entities::{Language, TranslationResult};
use crate::domain::errors::TranslationError;
use crate::domain::ports::TranslatorPort;

pub async fn translate_batch_sequential<T: TranslatorPort + ?Sized>(
    translator: &T,
    texts: &[String],
    source_lang: &Language,
    target_lang: &Language,
) -> Result<Vec<TranslationResult>, TranslationError> {
    let mut results = Vec::with_capacity(texts.len());
    for text in texts {
        let res = translator
            .translate_html(text, source_lang, target_lang)
            .await?;
        results.push(res);
    }
    Ok(results)
}
