use crate::domain::entities::{ChapterLatency, EpubBook, ExecutionMetrics, Language};
use crate::domain::errors::{AppError, ChapterTranslationError, TranslationError};
use crate::domain::ports::{CachePort, EpubPort, ProgressReporterPort, TranslatorPort};
use crate::domain::services::XhtmlValidator;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Semaphore};
use tokio::task::JoinSet;

const DEFAULT_CONCURRENCY: usize = 4;

#[derive(Clone, Debug, PartialEq)]
enum ChapterOutcome {
    Hit,
    Miss(Duration),
    Empty,
}

pub struct TranslateBookUseCase {
    epub_port: Arc<dyn EpubPort>,
    translator_port: Arc<dyn TranslatorPort>,
    cache_port: Arc<dyn CachePort>,
    progress_reporter: Option<Arc<dyn ProgressReporterPort>>,
    concurrency: usize,
}

impl TranslateBookUseCase {
    pub fn new(
        epub_port: Arc<dyn EpubPort>,
        translator_port: Arc<dyn TranslatorPort>,
        cache_port: Arc<dyn CachePort>,
        progress_reporter: Option<Arc<dyn ProgressReporterPort>>,
    ) -> Self {
        Self::with_concurrency(
            epub_port,
            translator_port,
            cache_port,
            progress_reporter,
            DEFAULT_CONCURRENCY,
        )
    }

    pub fn with_concurrency(
        epub_port: Arc<dyn EpubPort>,
        translator_port: Arc<dyn TranslatorPort>,
        cache_port: Arc<dyn CachePort>,
        progress_reporter: Option<Arc<dyn ProgressReporterPort>>,
        concurrency: usize,
    ) -> Self {
        Self {
            epub_port,
            translator_port,
            cache_port,
            progress_reporter,
            concurrency: concurrency.max(1),
        }
    }

    pub fn concurrency(&self) -> usize {
        self.concurrency
    }

    pub async fn execute(
        &self,
        input_path: &Path,
        output_path: &Path,
        source_lang: &Language,
        target_lang: &Language,
    ) -> Result<ExecutionMetrics, AppError> {
        let start_instant = Instant::now();
        let mut book = self.epub_port.read_epub(input_path).await?;
        let book_title = book.metadata.title.as_deref().unwrap_or_else(|| {
            input_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Livro")
        });

        let total_chapters = book.chapters.len();
        if let Some(reporter) = &self.progress_reporter {
            reporter.on_start(total_chapters, book_title);
        }

        let semaphore = Arc::new(Semaphore::new(self.concurrency));
        let current_source_lang = Arc::new(RwLock::new(source_lang.clone()));
        let provider_name = self.translator_port.provider_name().to_string();

        let mut join_set = JoinSet::new();

        for (index, chapter) in book.chapters.iter().enumerate() {
            let chapter_path = chapter.path.clone();
            let trimmed_content = chapter.content.trim().to_string();
            let semaphore = semaphore.clone();
            let translator_port = self.translator_port.clone();
            let cache_port = self.cache_port.clone();
            let progress_reporter = self.progress_reporter.clone();
            let current_source_lang = current_source_lang.clone();
            let target_lang = target_lang.clone();
            let provider_name = provider_name.clone();

            join_set.spawn(async move {
                let _permit = semaphore.acquire().await.map_err(|e| {
                    AppError::Translation(TranslationError::Process(format!(
                        "Failed to acquire semaphore permit: {e}"
                    )))
                })?;

                if let Some(reporter) = &progress_reporter {
                    reporter.on_chapter_start(index, &chapter_path);
                }

                if trimmed_content.is_empty() {
                    if let Some(reporter) = &progress_reporter {
                        reporter.on_chapter_complete(index, &chapter_path);
                    }
                    return Ok((index, String::new(), ChapterOutcome::Empty));
                }

                let cur_lang = {
                    let lock = current_source_lang.read().await;
                    lock.clone()
                };

                let cached_translation = cache_port
                    .get(
                        cur_lang.as_str(),
                        target_lang.as_str(),
                        &provider_name,
                        &trimmed_content,
                    )
                    .await?;

                let valid_cached = match cached_translation.filter(|c| !c.trim().is_empty()) {
                    Some(cached) => match XhtmlValidator::heal_and_validate(&cached) {
                        Ok(healed) => {
                            if healed != cached {
                                let _ = cache_port
                                    .set(
                                        cur_lang.as_str(),
                                        target_lang.as_str(),
                                        &provider_name,
                                        &trimmed_content,
                                        &healed,
                                    )
                                    .await;
                                if cur_lang.is_auto() {
                                    let _ = cache_port
                                        .set(
                                            "auto",
                                            target_lang.as_str(),
                                            &provider_name,
                                            &trimmed_content,
                                            &healed,
                                        )
                                        .await;
                                }
                            }
                            Some(healed)
                        }
                        Err(_) => None,
                    },
                    None => None,
                };

                match valid_cached {
                    Some(cached) => {
                        if let Some(reporter) = &progress_reporter {
                            reporter.on_chapter_complete(index, &chapter_path);
                        }
                        Ok((index, cached, ChapterOutcome::Hit))
                    }
                    None => {
                        let (preview_start, preview_end) = Self::extract_snippets(&trimmed_content);
                        let call_start = Instant::now();
                        let result = translator_port
                            .translate_html(&trimmed_content, &cur_lang, &target_lang)
                            .await
                            .map_err(|err| {
                                AppError::ChapterTranslation(Box::new(ChapterTranslationError {
                                    chapter_index: index,
                                    chapter_path: chapter_path.clone(),
                                    chapter_size: trimmed_content.len(),
                                    chapter_preview_start: preview_start.clone(),
                                    chapter_preview_end: preview_end.clone(),
                                    location: crate::here!(),
                                    source: err,
                                }))
                            })?;
                        let call_duration = call_start.elapsed();

                        let validated_text = XhtmlValidator::heal_and_validate(
                            &result.translated_text,
                        )
                        .map_err(|val_err| {
                            AppError::ChapterTranslation(Box::new(ChapterTranslationError {
                                chapter_index: index,
                                chapter_path: chapter_path.clone(),
                                chapter_size: result.translated_text.len(),
                                chapter_preview_start: preview_start,
                                chapter_preview_end: preview_end,
                                location: crate::here!(),
                                source: TranslationError::XmlValidation(val_err.to_string()),
                            }))
                        })?;

                        let was_auto = cur_lang.is_auto();
                        if was_auto {
                            cache_port
                                .set(
                                    "auto",
                                    target_lang.as_str(),
                                    &provider_name,
                                    &trimmed_content,
                                    &validated_text,
                                )
                                .await?;
                        }

                        if let Some(detected) = result.detected_source {
                            let mut lock = current_source_lang.write().await;
                            if lock.is_auto() {
                                *lock = detected.clone();
                            }
                            drop(lock);

                            cache_port
                                .set(
                                    detected.as_str(),
                                    target_lang.as_str(),
                                    &provider_name,
                                    &trimmed_content,
                                    &validated_text,
                                )
                                .await?;
                        } else {
                            cache_port
                                .set(
                                    cur_lang.as_str(),
                                    target_lang.as_str(),
                                    &provider_name,
                                    &trimmed_content,
                                    &validated_text,
                                )
                                .await?;
                        }

                        if let Some(reporter) = &progress_reporter {
                            reporter.on_chapter_complete(index, &chapter_path);
                        }

                        Ok((index, validated_text, ChapterOutcome::Miss(call_duration)))
                    }
                }
            });
        }

        let mut translated_contents: Vec<Option<String>> = vec![None; total_chapters];
        let mut chapter_outcomes: Vec<Option<ChapterOutcome>> = vec![None; total_chapters];

        while let Some(join_res) = join_set.join_next().await {
            match join_res {
                Ok(Ok((index, content, outcome))) => {
                    translated_contents[index] = Some(content);
                    chapter_outcomes[index] = Some(outcome);
                }
                Ok(Err(app_error)) => {
                    join_set.abort_all();
                    return Err(app_error);
                }
                Err(join_error) => {
                    join_set.abort_all();
                    return Err(AppError::Translation(TranslationError::Process(format!(
                        "Concurrent task join error: {join_error}"
                    ))));
                }
            }
        }

        for (index, chapter) in book.chapters.iter_mut().enumerate() {
            if let Some(content) = translated_contents[index].take() {
                chapter.content = content;
            }
        }

        book.metadata.language = Some(target_lang.clone());
        Self::update_opf_language(&mut book, target_lang);

        self.epub_port.write_epub(&book, output_path).await?;

        let mut cache_hits = 0;
        let mut cache_misses = 0;
        let mut empty_chapters = 0;
        let mut chapter_latencies = Vec::new();

        for (index, outcome_opt) in chapter_outcomes.into_iter().enumerate() {
            let Some(outcome) = outcome_opt else { continue };
            match outcome {
                ChapterOutcome::Hit => cache_hits += 1,
                ChapterOutcome::Miss(latency) => {
                    cache_misses += 1;
                    let path = book
                        .chapters
                        .get(index)
                        .map(|c| c.path.clone())
                        .unwrap_or_default();
                    chapter_latencies.push(ChapterLatency {
                        index,
                        path,
                        latency,
                    });
                }
                ChapterOutcome::Empty => empty_chapters += 1,
            }
        }

        let total_elapsed = start_instant.elapsed();
        let metrics = ExecutionMetrics {
            total_chapters,
            cache_hits,
            cache_misses,
            empty_chapters,
            chapter_latencies,
            total_elapsed,
        };

        self.cache_port
            .record_metrics(&provider_name, target_lang.as_str(), &metrics)
            .await?;

        if let Some(reporter) = &self.progress_reporter {
            reporter.on_finish("Tradução concluída com sucesso!");
            reporter.on_metrics(&metrics);
        }

        Ok(metrics)
    }

    fn extract_snippets(content: &str) -> (String, String) {
        let lines: Vec<&str> = content.lines().collect();
        let start_lines: Vec<&str> = lines.iter().take(15).copied().collect();
        let start = start_lines.join("\n");

        let end = if lines.len() > 15 {
            let skip_count = lines.len().saturating_sub(15);
            let end_lines: Vec<&str> = lines.iter().skip(skip_count).copied().collect();
            end_lines.join("\n")
        } else {
            start.clone()
        };

        (start, end)
    }

    fn update_opf_language(book: &mut EpubBook, target_lang: &Language) {
        if let Some(opf_content) = &book.raw_opf_content {
            let updated = update_xml_tag(opf_content, "dc:language", target_lang.as_str());
            book.raw_opf_content = Some(updated);
        }
    }
}

fn update_xml_tag(xml: &str, tag_name: &str, new_value: &str) -> String {
    let open_tag = format!("<{tag_name}>");
    let close_tag = format!("</{tag_name}>");

    if let Some(start_pos) = xml.find(&open_tag)
        && let Some(end_pos) = xml[start_pos..].find(&close_tag)
    {
        let actual_end = start_pos + end_pos + close_tag.len();
        let mut result = String::with_capacity(xml.len() + new_value.len());
        result.push_str(&xml[..start_pos]);
        result.push_str(&open_tag);
        result.push_str(new_value);
        result.push_str(&close_tag);
        result.push_str(&xml[actual_end..]);
        return result;
    }

    xml.to_string()
}
