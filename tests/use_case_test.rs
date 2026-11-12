use async_trait::async_trait;
use epub_translator_google::adapters::cache::SqliteCacheAdapter;
use epub_translator_google::domain::entities::{
    EpubBook, EpubChapter, EpubMetadata, ExecutionMetrics, Language, TranslationResult,
};
use epub_translator_google::domain::errors::{AppError, EpubError, TranslationError};
use epub_translator_google::domain::ports::{
    CachePort, EpubPort, ProgressReporterPort, TranslatorPort,
};
use epub_translator_google::domain::services::TranslateBookUseCase;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::Mutex;

struct MockTranslator {
    call_count: AtomicUsize,
}

#[async_trait]
impl TranslatorPort for MockTranslator {
    async fn translate_html(
        &self,
        html: &str,
        _source_lang: &Language,
        _target_lang: &Language,
    ) -> Result<TranslationResult, TranslationError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let translated = format!("<translated>{html}</translated>");
        Ok(TranslationResult {
            translated_text: translated,
            detected_source: Some(Language::new("en")),
        })
    }

    async fn translate_batch(
        &self,
        texts: &[String],
        source_lang: &Language,
        target_lang: &Language,
    ) -> Result<Vec<TranslationResult>, TranslationError> {
        let mut results = Vec::new();
        for text in texts {
            results.push(self.translate_html(text, source_lang, target_lang).await?);
        }
        Ok(results)
    }

    fn provider_name(&self) -> &'static str {
        "mock-translator"
    }
}

struct ConcurrentMockTranslator {
    active_count: AtomicUsize,
    max_observed_concurrency: AtomicUsize,
    total_calls: AtomicUsize,
    delay_ms: u64,
}

impl ConcurrentMockTranslator {
    fn new(delay_ms: u64) -> Self {
        Self {
            active_count: AtomicUsize::new(0),
            max_observed_concurrency: AtomicUsize::new(0),
            total_calls: AtomicUsize::new(0),
            delay_ms,
        }
    }
}

#[async_trait]
impl TranslatorPort for ConcurrentMockTranslator {
    async fn translate_html(
        &self,
        html: &str,
        _source_lang: &Language,
        _target_lang: &Language,
    ) -> Result<TranslationResult, TranslationError> {
        let current = self.active_count.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_observed_concurrency
            .fetch_max(current, Ordering::SeqCst);
        if self.delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
        }
        self.active_count.fetch_sub(1, Ordering::SeqCst);
        self.total_calls.fetch_add(1, Ordering::SeqCst);

        Ok(TranslationResult {
            translated_text: format!("<translated>{html}</translated>"),
            detected_source: Some(Language::new("en")),
        })
    }

    async fn translate_batch(
        &self,
        texts: &[String],
        source_lang: &Language,
        target_lang: &Language,
    ) -> Result<Vec<TranslationResult>, TranslationError> {
        let mut results = Vec::new();
        for text in texts {
            results.push(self.translate_html(text, source_lang, target_lang).await?);
        }
        Ok(results)
    }

    fn provider_name(&self) -> &'static str {
        "concurrent-mock-translator"
    }
}

struct OrderScramblingTranslator;

#[async_trait]
impl TranslatorPort for OrderScramblingTranslator {
    async fn translate_html(
        &self,
        html: &str,
        _source_lang: &Language,
        _target_lang: &Language,
    ) -> Result<TranslationResult, TranslationError> {
        let delay_ms = if html.contains("Chapter 0") || html.contains("Chapter 4") {
            40
        } else if html.contains("Chapter 1") || html.contains("Chapter 3") {
            20
        } else {
            5
        };
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;

        Ok(TranslationResult {
            translated_text: format!("<translated>{html}</translated>"),
            detected_source: Some(Language::new("en")),
        })
    }

    async fn translate_batch(
        &self,
        texts: &[String],
        source_lang: &Language,
        target_lang: &Language,
    ) -> Result<Vec<TranslationResult>, TranslationError> {
        let mut results = Vec::new();
        for text in texts {
            results.push(self.translate_html(text, source_lang, target_lang).await?);
        }
        Ok(results)
    }

    fn provider_name(&self) -> &'static str {
        "order-scrambling-translator"
    }
}

struct FailingTranslator {
    fail_on_index: usize,
}

#[async_trait]
impl TranslatorPort for FailingTranslator {
    async fn translate_html(
        &self,
        html: &str,
        _source_lang: &Language,
        _target_lang: &Language,
    ) -> Result<TranslationResult, TranslationError> {
        if html.contains(&format!("Chapter {}", self.fail_on_index)) {
            return Err(TranslationError::Process(
                "Simulated persistent failure".to_string(),
            ));
        }

        Ok(TranslationResult {
            translated_text: format!("<translated>{html}</translated>"),
            detected_source: Some(Language::new("en")),
        })
    }

    async fn translate_batch(
        &self,
        texts: &[String],
        source_lang: &Language,
        target_lang: &Language,
    ) -> Result<Vec<TranslationResult>, TranslationError> {
        let mut results = Vec::new();
        for text in texts {
            results.push(self.translate_html(text, source_lang, target_lang).await?);
        }
        Ok(results)
    }

    fn provider_name(&self) -> &'static str {
        "failing-translator"
    }
}

struct MockEpubPort {
    book: Mutex<EpubBook>,
    written_book: Mutex<Option<EpubBook>>,
}

#[async_trait]
impl EpubPort for MockEpubPort {
    async fn read_epub(&self, _path: &Path) -> Result<EpubBook, EpubError> {
        let lock = self.book.lock().await;
        Ok(lock.clone())
    }

    async fn write_epub(&self, book: &EpubBook, _path: &Path) -> Result<(), EpubError> {
        let mut lock = self.written_book.lock().await;
        *lock = Some(book.clone());
        Ok(())
    }

    async fn verify_epub(
        &self,
        _path: &Path,
    ) -> Result<epub_translator_google::domain::entities::EpubVerificationReport, EpubError> {
        Ok(
            epub_translator_google::domain::entities::EpubVerificationReport {
                is_valid: true,
                mimetype_valid: true,
                container_xml_valid: true,
                opf_valid: true,
                total_xhtml_files: 1,
                valid_xhtml_files: 1,
                errors: Vec::new(),
            },
        )
    }
}

struct MockProgressReporter {
    started: AtomicUsize,
    chapters_completed: AtomicUsize,
    finished: AtomicUsize,
    metrics_received: std::sync::Mutex<Option<ExecutionMetrics>>,
}

impl MockProgressReporter {
    fn new() -> Self {
        Self {
            started: AtomicUsize::new(0),
            chapters_completed: AtomicUsize::new(0),
            finished: AtomicUsize::new(0),
            metrics_received: std::sync::Mutex::new(None),
        }
    }
}

impl ProgressReporterPort for MockProgressReporter {
    fn on_start(&self, _total_chapters: usize, _book_title: &str) {
        self.started.fetch_add(1, Ordering::SeqCst);
    }

    fn on_chapter_start(&self, _index: usize, _chapter_path: &str) {}

    fn on_chapter_complete(&self, _index: usize, _chapter_path: &str) {
        self.chapters_completed.fetch_add(1, Ordering::SeqCst);
    }

    fn on_finish(&self, _message: &str) {
        self.finished.fetch_add(1, Ordering::SeqCst);
    }

    fn on_metrics(&self, metrics: &ExecutionMetrics) {
        let mut lock = self.metrics_received.lock().unwrap();
        *lock = Some(metrics.clone());
    }
}

#[tokio::test]
async fn test_translate_book_use_case_orchestration() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join("test_use_case_cache.db");
    if db_path.exists() {
        let _ = fs::remove_file(&db_path);
    }

    let cache_adapter = Arc::new(SqliteCacheAdapter::new(&db_path).unwrap());
    let translator_adapter = Arc::new(MockTranslator {
        call_count: AtomicUsize::new(0),
    });

    let sample_chapter = EpubChapter {
        path: "OEBPS/ch1.xhtml".to_string(),
        content: "<p>Hello</p>".to_string(),
        media_type: "application/xhtml+xml".to_string(),
    };

    let sample_book = EpubBook {
        metadata: EpubMetadata {
            title: Some("My Book".to_string()),
            language: Some(Language::new("en")),
            creator: None,
            identifier: None,
            publisher: None,
        },
        chapters: vec![sample_chapter],
        resources: Vec::new(),
        raw_container_xml: Some("<container/>".to_string()),
        raw_opf_content: Some("<package><dc:language>en</dc:language></package>".to_string()),
        opf_path: "OEBPS/content.opf".to_string(),
    };

    let epub_port = Arc::new(MockEpubPort {
        book: Mutex::new(sample_book),
        written_book: Mutex::new(None),
    });

    let progress_reporter = Arc::new(MockProgressReporter::new());

    let use_case = TranslateBookUseCase::new(
        epub_port.clone(),
        translator_adapter.clone(),
        cache_adapter.clone(),
        Some(progress_reporter.clone()),
    );

    let dummy_in = Path::new("dummy_in.epub");
    let dummy_out = Path::new("dummy_out.epub");
    let source_lang = Language::auto();
    let target_lang = Language::new("pt-BR");

    let first_metrics = use_case
        .execute(dummy_in, dummy_out, &source_lang, &target_lang)
        .await
        .unwrap();

    assert_eq!(first_metrics.total_chapters, 1);
    assert_eq!(first_metrics.cache_hits, 0);
    assert_eq!(first_metrics.cache_misses, 1);
    assert_eq!(first_metrics.cache_hit_ratio(), 0.0);
    assert_eq!(first_metrics.chapter_latencies.len(), 1);
    assert_eq!(first_metrics.chapter_latencies[0].index, 0);
    assert_eq!(first_metrics.chapter_latencies[0].path, "OEBPS/ch1.xhtml");
    assert!(first_metrics.total_elapsed > Duration::ZERO);

    assert_eq!(translator_adapter.call_count.load(Ordering::SeqCst), 1);
    assert_eq!(progress_reporter.started.load(Ordering::SeqCst), 1);
    assert_eq!(
        progress_reporter.chapters_completed.load(Ordering::SeqCst),
        1
    );
    assert_eq!(progress_reporter.finished.load(Ordering::SeqCst), 1);

    {
        let recorded = progress_reporter.metrics_received.lock().unwrap();
        assert_eq!(recorded.as_ref(), Some(&first_metrics));
    }

    let written = epub_port.written_book.lock().await;
    let final_book = written.as_ref().unwrap();
    assert_eq!(
        final_book.metadata.language.as_ref().unwrap().as_str(),
        "pt-BR"
    );
    assert_eq!(
        final_book.chapters[0].content,
        "<translated><p>Hello</p></translated>"
    );
    assert!(
        final_book
            .raw_opf_content
            .as_ref()
            .unwrap()
            .contains("<dc:language>pt-BR</dc:language>")
    );
    drop(written);

    let second_metrics = use_case
        .execute(dummy_in, dummy_out, &source_lang, &target_lang)
        .await
        .unwrap();

    assert_eq!(translator_adapter.call_count.load(Ordering::SeqCst), 1);
    assert_eq!(second_metrics.total_chapters, 1);
    assert_eq!(second_metrics.cache_hits, 1);
    assert_eq!(second_metrics.cache_misses, 0);
    assert_eq!(second_metrics.cache_hit_ratio(), 100.0);
    assert!(second_metrics.chapter_latencies.is_empty());

    let history = cache_adapter.get_metrics_history(10).await.unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].cache_hits, 1);
    assert_eq!(history[0].cache_misses, 0);
    assert_eq!(history[1].cache_hits, 0);
    assert_eq!(history[1].cache_misses, 1);

    let _ = fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_translate_book_use_case_concurrency_bounded_by_semaphore() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join("test_use_case_concurrency_cache.db");
    if db_path.exists() {
        let _ = fs::remove_file(&db_path);
    }

    let cache_adapter = Arc::new(SqliteCacheAdapter::new(&db_path).unwrap());
    let translator_adapter = Arc::new(ConcurrentMockTranslator::new(30));

    let chapters: Vec<EpubChapter> = (0..8)
        .map(|i| EpubChapter {
            path: format!("OEBPS/ch{i}.xhtml"),
            content: format!("<p>Chapter {i} text</p>"),
            media_type: "application/xhtml+xml".to_string(),
        })
        .collect();

    let sample_book = EpubBook {
        metadata: EpubMetadata {
            title: Some("Concurrent Book".to_string()),
            language: Some(Language::new("en")),
            creator: None,
            identifier: None,
            publisher: None,
        },
        chapters,
        resources: Vec::new(),
        raw_container_xml: Some("<container/>".to_string()),
        raw_opf_content: Some("<package><dc:language>en</dc:language></package>".to_string()),
        opf_path: "OEBPS/content.opf".to_string(),
    };

    let epub_port = Arc::new(MockEpubPort {
        book: Mutex::new(sample_book),
        written_book: Mutex::new(None),
    });

    let progress_reporter = Arc::new(MockProgressReporter::new());

    let concurrency_limit = 3;
    let use_case = TranslateBookUseCase::with_concurrency(
        epub_port.clone(),
        translator_adapter.clone(),
        cache_adapter.clone(),
        Some(progress_reporter.clone()),
        concurrency_limit,
    );

    assert_eq!(use_case.concurrency(), concurrency_limit);

    let dummy_in = Path::new("dummy_in.epub");
    let dummy_out = Path::new("dummy_out.epub");
    let source_lang = Language::new("en");
    let target_lang = Language::new("pt-BR");

    use_case
        .execute(dummy_in, dummy_out, &source_lang, &target_lang)
        .await
        .unwrap();

    let max_concurrency = translator_adapter
        .max_observed_concurrency
        .load(Ordering::SeqCst);
    assert!(max_concurrency <= concurrency_limit);
    assert!(max_concurrency > 1);
    assert_eq!(translator_adapter.total_calls.load(Ordering::SeqCst), 8);
    assert_eq!(
        progress_reporter.chapters_completed.load(Ordering::SeqCst),
        8
    );
    assert_eq!(progress_reporter.finished.load(Ordering::SeqCst), 1);

    let _ = fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_translate_book_use_case_preserves_deterministic_chapter_order() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join("test_use_case_order_cache.db");
    if db_path.exists() {
        let _ = fs::remove_file(&db_path);
    }

    let cache_adapter = Arc::new(SqliteCacheAdapter::new(&db_path).unwrap());
    let translator_adapter = Arc::new(OrderScramblingTranslator);

    let chapter_count = 6;
    let chapters: Vec<EpubChapter> = (0..chapter_count)
        .map(|i| EpubChapter {
            path: format!("OEBPS/ch{i}.xhtml"),
            content: format!("<p>Chapter {i} distinct body</p>"),
            media_type: "application/xhtml+xml".to_string(),
        })
        .collect();

    let sample_book = EpubBook {
        metadata: EpubMetadata {
            title: Some("Order Book".to_string()),
            language: Some(Language::new("en")),
            creator: None,
            identifier: None,
            publisher: None,
        },
        chapters,
        resources: Vec::new(),
        raw_container_xml: Some("<container/>".to_string()),
        raw_opf_content: Some("<package><dc:language>en</dc:language></package>".to_string()),
        opf_path: "OEBPS/content.opf".to_string(),
    };

    let epub_port = Arc::new(MockEpubPort {
        book: Mutex::new(sample_book),
        written_book: Mutex::new(None),
    });

    let use_case = TranslateBookUseCase::with_concurrency(
        epub_port.clone(),
        translator_adapter,
        cache_adapter,
        None,
        4,
    );

    let dummy_in = Path::new("dummy_in.epub");
    let dummy_out = Path::new("dummy_out.epub");
    let source_lang = Language::new("en");
    let target_lang = Language::new("pt-BR");

    use_case
        .execute(dummy_in, dummy_out, &source_lang, &target_lang)
        .await
        .unwrap();

    let written = epub_port.written_book.lock().await;
    let final_book = written.as_ref().unwrap();

    assert_eq!(final_book.chapters.len(), chapter_count);
    for i in 0..chapter_count {
        assert_eq!(final_book.chapters[i].path, format!("OEBPS/ch{i}.xhtml"));
        assert_eq!(
            final_book.chapters[i].content,
            format!("<translated><p>Chapter {i} distinct body</p></translated>")
        );
    }

    let _ = fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_translate_book_use_case_concurrent_error_propagation() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join("test_use_case_error_cache.db");
    if db_path.exists() {
        let _ = fs::remove_file(&db_path);
    }

    let cache_adapter = Arc::new(SqliteCacheAdapter::new(&db_path).unwrap());
    let translator_adapter = Arc::new(FailingTranslator { fail_on_index: 2 });

    let chapters: Vec<EpubChapter> = (0..5)
        .map(|i| EpubChapter {
            path: format!("OEBPS/ch{i}.xhtml"),
            content: format!("<p>Chapter {i}</p>"),
            media_type: "application/xhtml+xml".to_string(),
        })
        .collect();

    let sample_book = EpubBook {
        metadata: EpubMetadata {
            title: Some("Error Book".to_string()),
            language: Some(Language::new("en")),
            creator: None,
            identifier: None,
            publisher: None,
        },
        chapters,
        resources: Vec::new(),
        raw_container_xml: Some("<container/>".to_string()),
        raw_opf_content: Some("<package><dc:language>en</dc:language></package>".to_string()),
        opf_path: "OEBPS/content.opf".to_string(),
    };

    let epub_port = Arc::new(MockEpubPort {
        book: Mutex::new(sample_book),
        written_book: Mutex::new(None),
    });

    let use_case = TranslateBookUseCase::with_concurrency(
        epub_port.clone(),
        translator_adapter,
        cache_adapter,
        None,
        3,
    );

    let dummy_in = Path::new("dummy_in.epub");
    let dummy_out = Path::new("dummy_out.epub");
    let source_lang = Language::new("en");
    let target_lang = Language::new("pt-BR");

    let result = use_case
        .execute(dummy_in, dummy_out, &source_lang, &target_lang)
        .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AppError::ChapterTranslation(boxed) => {
            assert_eq!(boxed.chapter_index, 2);
            assert_eq!(boxed.chapter_path, "OEBPS/ch2.xhtml");
        }
        other => panic!("Expected ChapterTranslation error, got: {:?}", other),
    }

    let _ = fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_translate_book_use_case_concurrency_boundary_value_zero_clamps_to_one() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join("test_use_case_bva_zero.db");
    if db_path.exists() {
        let _ = fs::remove_file(&db_path);
    }

    let cache_adapter = Arc::new(SqliteCacheAdapter::new(&db_path).unwrap());
    let translator_adapter = Arc::new(MockTranslator {
        call_count: AtomicUsize::new(0),
    });

    let sample_chapter = EpubChapter {
        path: "OEBPS/ch1.xhtml".to_string(),
        content: "<p>BVA Zero</p>".to_string(),
        media_type: "application/xhtml+xml".to_string(),
    };

    let sample_book = EpubBook {
        metadata: EpubMetadata {
            title: Some("BVA Zero Book".to_string()),
            language: Some(Language::new("en")),
            creator: None,
            identifier: None,
            publisher: None,
        },
        chapters: vec![sample_chapter],
        resources: Vec::new(),
        raw_container_xml: Some("<container/>".to_string()),
        raw_opf_content: Some("<package><dc:language>en</dc:language></package>".to_string()),
        opf_path: "OEBPS/content.opf".to_string(),
    };

    let epub_port = Arc::new(MockEpubPort {
        book: Mutex::new(sample_book),
        written_book: Mutex::new(None),
    });

    let use_case = TranslateBookUseCase::with_concurrency(
        epub_port.clone(),
        translator_adapter.clone(),
        cache_adapter.clone(),
        None,
        0,
    );

    assert_eq!(use_case.concurrency(), 1);

    let dummy_in = Path::new("dummy_in.epub");
    let dummy_out = Path::new("dummy_out.epub");
    let source_lang = Language::auto();
    let target_lang = Language::new("pt-BR");

    let metrics = use_case
        .execute(dummy_in, dummy_out, &source_lang, &target_lang)
        .await
        .unwrap();

    assert_eq!(metrics.total_chapters, 1);
    assert_eq!(metrics.cache_misses, 1);
    assert_eq!(translator_adapter.call_count.load(Ordering::SeqCst), 1);

    let _ = fs::remove_file(&db_path);
}
