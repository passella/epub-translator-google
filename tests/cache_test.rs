use epub_translator_google::adapters::cache::{SqliteCacheAdapter, SqliteTranslationCacheAdapter};
use epub_translator_google::domain::entities::{ChapterLatency, ExecutionMetrics};
use epub_translator_google::domain::ports::CachePort;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

#[tokio::test]
async fn test_sqlite_cache_lifecycle() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join("test_translations_cache_temp.db");
    if db_path.exists() {
        let _ = fs::remove_file(&db_path);
    }

    let cache = SqliteCacheAdapter::new(&db_path).unwrap();

    let miss = cache
        .get("en", "pt-BR", "google-free", "Hello world")
        .await
        .unwrap();
    assert_eq!(miss, None);

    cache
        .set("en", "pt-BR", "google-free", "Hello world", "Olá mundo")
        .await
        .unwrap();

    let hit = cache
        .get("en", "pt-BR", "google-free", "Hello world")
        .await
        .unwrap();
    assert_eq!(hit, Some("Olá mundo".to_string()));

    let diff_provider = cache
        .get("en", "pt-BR", "antigravity", "Hello world")
        .await
        .unwrap();
    assert_eq!(diff_provider, None);

    cache
        .set(
            "en",
            "pt-BR",
            "google-free",
            "Hello world",
            "Olá mundo atualizado",
        )
        .await
        .unwrap();

    let updated_hit = cache
        .get("en", "pt-BR", "google-free", "Hello world")
        .await
        .unwrap();
    assert_eq!(updated_hit, Some("Olá mundo atualizado".to_string()));

    let _ = fs::remove_file(&db_path);
    let wal = PathBuf::from(format!("{}-wal", db_path.display()));
    let shm = PathBuf::from(format!("{}-shm", db_path.display()));
    let _ = fs::remove_file(wal);
    let _ = fs::remove_file(shm);
}

#[tokio::test]
async fn test_sqlite_block_cache_lifecycle() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join("test_block_cache_lifecycle.db");
    if db_path.exists() {
        let _ = fs::remove_file(&db_path);
    }

    let cache = SqliteTranslationCacheAdapter::new(&db_path).unwrap();

    let miss = cache
        .get_block("en", "pt-BR", "antigravity", "<p>Block text</p>")
        .await
        .unwrap();
    assert_eq!(miss, None);

    cache
        .set_block(
            "en",
            "pt-BR",
            "antigravity",
            "<p>Block text</p>",
            "<p>Texto do bloco</p>",
        )
        .await
        .unwrap();

    let hit = cache
        .get_block("en", "pt-BR", "antigravity", "<p>Block text</p>")
        .await
        .unwrap();
    assert_eq!(hit, Some("<p>Texto do bloco</p>".to_string()));

    cache
        .set_block(
            "en",
            "pt-BR",
            "antigravity",
            "<p>Block text</p>",
            "<p>Texto do bloco revisado</p>",
        )
        .await
        .unwrap();

    let updated_hit = cache
        .get_block("en", "pt-BR", "antigravity", "<p>Block text</p>")
        .await
        .unwrap();
    assert_eq!(
        updated_hit,
        Some("<p>Texto do bloco revisado</p>".to_string())
    );

    let _ = fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_sqlite_block_cache_deterministic_key_isolation() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join("test_block_cache_isolation.db");
    if db_path.exists() {
        let _ = fs::remove_file(&db_path);
    }

    let cache = SqliteCacheAdapter::new(&db_path).unwrap();

    cache
        .set_block(
            "en",
            "pt-BR",
            "antigravity",
            "<p>Specific block</p>",
            "<p>Bloco específico</p>",
        )
        .await
        .unwrap();

    let diff_src = cache
        .get_block("fr", "pt-BR", "antigravity", "<p>Specific block</p>")
        .await
        .unwrap();
    assert_eq!(diff_src, None);

    let diff_target = cache
        .get_block("en", "es", "antigravity", "<p>Specific block</p>")
        .await
        .unwrap();
    assert_eq!(diff_target, None);

    let diff_provider = cache
        .get_block("en", "pt-BR", "google-cloud", "<p>Specific block</p>")
        .await
        .unwrap();
    assert_eq!(diff_provider, None);

    let diff_text = cache
        .get_block("en", "pt-BR", "antigravity", "<p>Other block</p>")
        .await
        .unwrap();
    assert_eq!(diff_text, None);

    let _ = fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_sqlite_chapter_and_block_cache_coexistence() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join("test_cache_coexistence.db");
    if db_path.exists() {
        let _ = fs::remove_file(&db_path);
    }

    let cache = SqliteCacheAdapter::new(&db_path).unwrap();
    let text = "<p>Identical text</p>";

    cache
        .set(
            "en",
            "pt-BR",
            "antigravity",
            text,
            "<p>Capitulo traduzido</p>",
        )
        .await
        .unwrap();

    cache
        .set_block("en", "pt-BR", "antigravity", text, "<p>Bloco traduzido</p>")
        .await
        .unwrap();

    let chapter_res = cache.get("en", "pt-BR", "antigravity", text).await.unwrap();
    let block_res = cache
        .get_block("en", "pt-BR", "antigravity", text)
        .await
        .unwrap();

    assert_eq!(chapter_res, Some("<p>Capitulo traduzido</p>".to_string()));
    assert_eq!(block_res, Some("<p>Bloco traduzido</p>".to_string()));

    let _ = fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_sqlite_execution_metrics_history_empty_and_retrieval() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join("test_metrics_empty.db");
    if db_path.exists() {
        let _ = fs::remove_file(&db_path);
    }

    let cache = SqliteCacheAdapter::new(&db_path).unwrap();
    let history = cache.get_metrics_history(10).await.unwrap();
    assert!(history.is_empty());

    let _ = fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_sqlite_execution_metrics_history_single_record() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join("test_metrics_single.db");
    if db_path.exists() {
        let _ = fs::remove_file(&db_path);
    }

    let cache = SqliteCacheAdapter::new(&db_path).unwrap();

    let metrics = ExecutionMetrics {
        total_chapters: 10,
        cache_hits: 8,
        cache_misses: 2,
        empty_chapters: 0,
        chapter_latencies: vec![
            ChapterLatency {
                index: 0,
                path: "ch1.xhtml".to_string(),
                latency: Duration::from_millis(500),
            },
            ChapterLatency {
                index: 1,
                path: "ch2.xhtml".to_string(),
                latency: Duration::from_millis(700),
            },
        ],
        total_elapsed: Duration::from_millis(3500),
    };

    cache
        .record_metrics("antigravity", "pt-BR", &metrics)
        .await
        .unwrap();

    let history = cache.get_metrics_history(10).await.unwrap();
    assert_eq!(history.len(), 1);

    let rec = &history[0];
    assert_eq!(rec.id, 1);
    assert!(!rec.timestamp.is_empty());
    assert_eq!(rec.provider, "antigravity");
    assert_eq!(rec.target_lang, "pt-BR");
    assert_eq!(rec.total_chapters, 10);
    assert_eq!(rec.cache_hits, 8);
    assert_eq!(rec.cache_misses, 2);
    assert!((rec.hit_ratio_pct - 80.0).abs() < f64::EPSILON);
    assert!((rec.avg_latency_ms - 600.0).abs() < 1.0);
    assert!((rec.total_duration_secs - 3.5).abs() < 0.1);

    let _ = fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_sqlite_execution_metrics_history_multiple_rows_and_limit() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join("test_metrics_multi.db");
    if db_path.exists() {
        let _ = fs::remove_file(&db_path);
    }

    let cache = SqliteCacheAdapter::new(&db_path).unwrap();

    for i in 1..=5 {
        let metrics = ExecutionMetrics {
            total_chapters: i * 5,
            cache_hits: i * 4,
            cache_misses: i,
            empty_chapters: 0,
            chapter_latencies: Vec::new(),
            total_elapsed: Duration::from_secs(i as u64),
        };
        cache
            .record_metrics("antigravity", "pt-BR", &metrics)
            .await
            .unwrap();
    }

    let full_history = cache.get_metrics_history(10).await.unwrap();
    assert_eq!(full_history.len(), 5);
    assert_eq!(full_history[0].id, 5);
    assert_eq!(full_history[4].id, 1);

    let limited_history = cache.get_metrics_history(2).await.unwrap();
    assert_eq!(limited_history.len(), 2);
    assert_eq!(limited_history[0].id, 5);
    assert_eq!(limited_history[1].id, 4);

    let _ = fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_sqlite_cache_zstd_compression_and_transparent_read() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join("test_zstd_compression.db");
    if db_path.exists() {
        let _ = fs::remove_file(&db_path);
    }

    let cache = SqliteCacheAdapter::new(&db_path).unwrap();
    let long_text =
        "<p>Parágrafo longo de teste repetido para validar compressão ZSTD eficiente. ".repeat(50);

    cache
        .set("en", "pt-BR", "antigravity", "orig_key", &long_text)
        .await
        .unwrap();

    let raw_conn = rusqlite::Connection::open(&db_path).unwrap();
    let mut stmt = raw_conn
        .prepare("SELECT typeof(translated_text), translated_text FROM translations LIMIT 1")
        .unwrap();
    let (col_type, col_blob): (String, Vec<u8>) =
        stmt.query_row([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();

    assert_eq!(col_type, "blob");
    const ZSTD_MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];
    assert!(col_blob.len() >= 4);
    assert_eq!(&col_blob[..4], &ZSTD_MAGIC);
    assert!(col_blob.len() < long_text.len());

    let fetched = cache
        .get("en", "pt-BR", "antigravity", "orig_key")
        .await
        .unwrap();
    assert_eq!(fetched, Some(long_text));

    let _ = fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_sqlite_cache_retrocompatibility_with_uncompressed_text() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join("test_retrocompatibility.db");
    if db_path.exists() {
        let _ = fs::remove_file(&db_path);
    }

    {
        let raw_conn = rusqlite::Connection::open(&db_path).unwrap();
        raw_conn
            .execute_batch(
                "CREATE TABLE translations (
                    hash TEXT PRIMARY KEY,
                    source_lang TEXT NOT NULL,
                    target_lang TEXT NOT NULL,
                    provider TEXT NOT NULL,
                    original_text TEXT NOT NULL,
                    translated_text TEXT NOT NULL,
                    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
                );",
            )
            .unwrap();
    }

    let cache = SqliteCacheAdapter::new(&db_path).unwrap();

    let hash = SqliteCacheAdapter::calculate_hash("en", "pt-BR", "google-free", "legacy_key");

    {
        let raw_conn = rusqlite::Connection::open(&db_path).unwrap();
        raw_conn
            .execute(
                "INSERT INTO translations (hash, source_lang, target_lang, provider, original_text, translated_text)
                 VALUES (?1, 'en', 'pt-BR', 'google-free', 'legacy_key', 'Texto não comprimido herdado da v0.1.0')",
                rusqlite::params![hash],
            )
            .unwrap();
    }

    let fetched = cache
        .get("en", "pt-BR", "google-free", "legacy_key")
        .await
        .unwrap();
    assert_eq!(
        fetched,
        Some("Texto não comprimido herdado da v0.1.0".to_string())
    );

    let _ = fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_sqlite_cache_purges_empty_and_corrupted_records() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join("test_cache_purge.db");
    if db_path.exists() {
        let _ = fs::remove_file(&db_path);
    }

    let cache = SqliteCacheAdapter::new(&db_path).unwrap();

    {
        let raw_conn = rusqlite::Connection::open(&db_path).unwrap();
        raw_conn
            .execute(
                "INSERT INTO translations (hash, source_lang, target_lang, provider, original_text, translated_text)
                 VALUES ('h1', 'en', 'pt-BR', 'agy', 'k1', ''),
                        ('h2', 'en', 'pt-BR', 'agy', 'k2', '   \t\n  '),
                        ('h3', 'en', 'pt-BR', 'agy', 'k3', zeroblob(0));",
                [],
            )
            .unwrap();
    }

    let purged = cache.purge_corrupted_entries().await.unwrap();
    assert_eq!(purged, 3);

    {
        let raw_conn = rusqlite::Connection::open(&db_path).unwrap();
        raw_conn
            .execute(
                "INSERT INTO translations (hash, source_lang, target_lang, provider, original_text, translated_text)
                 VALUES ('h4', 'en', 'pt-BR', 'agy', 'k4', '');",
                [],
            )
            .unwrap();
    }

    drop(cache);
    let reopened = SqliteCacheAdapter::new(&db_path).unwrap();

    let raw_conn = rusqlite::Connection::open(&db_path).unwrap();
    let count: i64 = raw_conn
        .query_row("SELECT count(*) FROM translations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);

    drop(reopened);
    let _ = fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_sqlite_cache_refuses_to_cache_empty_text() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join("test_cache_refuse_empty.db");
    if db_path.exists() {
        let _ = fs::remove_file(&db_path);
    }

    let cache = SqliteCacheAdapter::new(&db_path).unwrap();

    let err_empty = cache.set("en", "pt-BR", "agy", "key", "").await;
    assert!(err_empty.is_err());

    let err_ws = cache.set("en", "pt-BR", "agy", "key", "   \n\t  ").await;
    assert!(err_ws.is_err());

    let err_block_empty = cache.set_block("en", "pt-BR", "agy", "key", "").await;
    assert!(err_block_empty.is_err());

    let err_block_ws = cache
        .set_block("en", "pt-BR", "agy", "key", "   \r\n  ")
        .await;
    assert!(err_block_ws.is_err());

    let _ = fs::remove_file(&db_path);
}
