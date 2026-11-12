use crate::domain::entities::{ExecutionMetrics, ExecutionMetricsRecord};
use crate::domain::errors::CacheError;
use crate::domain::ports::TranslationCachePort;
use async_trait::async_trait;
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::{Arc, Mutex};

pub struct SqliteCacheAdapter {
    conn: Arc<Mutex<Connection>>,
}

pub use SqliteCacheAdapter as SqliteTranslationCacheAdapter;

impl SqliteCacheAdapter {
    pub fn new(db_path: impl AsRef<Path>) -> Result<Self, CacheError> {
        let conn = Connection::open(db_path)
            .map_err(|e| CacheError::Database(format!("Failed to open SQLite database: {e}")))?;

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;
             CREATE TABLE IF NOT EXISTS translations (
                 hash TEXT PRIMARY KEY,
                 source_lang TEXT NOT NULL,
                 target_lang TEXT NOT NULL,
                 provider TEXT NOT NULL,
                 original_text TEXT NOT NULL,
                 translated_text TEXT NOT NULL,
                 created_at DATETIME DEFAULT CURRENT_TIMESTAMP
             );
             CREATE INDEX IF NOT EXISTS idx_lookup ON translations (source_lang, target_lang, provider, hash);
             CREATE TABLE IF NOT EXISTS execution_metrics_history (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
                 provider TEXT NOT NULL,
                 target_lang TEXT NOT NULL,
                 total_chapters INTEGER NOT NULL,
                 cache_hits INTEGER NOT NULL,
                 cache_misses INTEGER NOT NULL,
                 hit_ratio_pct REAL NOT NULL,
                 avg_latency_ms REAL NOT NULL,
                 total_duration_secs REAL NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_metrics_timestamp ON execution_metrics_history (timestamp);
             DELETE FROM translations WHERE length(translated_text) = 0 OR (typeof(translated_text) = 'text' AND trim(translated_text, ' ' || char(9) || char(10) || char(13)) = '');",
        )
        .map_err(|e| CacheError::Database(format!("Failed to initialize database tables: {e}")))?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub async fn purge_corrupted_entries(&self) -> Result<usize, CacheError> {
        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|e| CacheError::Database(format!("Database lock poisoned: {e}")))?;
            let count = conn
                .execute(
                    "DELETE FROM translations WHERE length(translated_text) = 0 OR (typeof(translated_text) = 'text' AND trim(translated_text, ' ' || char(9) || char(10) || char(13)) = '');",
                    [],
                )
                .map_err(|e| CacheError::Database(format!("Failed to purge corrupted cache entries: {e}")))?;
            Ok(count)
        })
        .await
        .map_err(|e| CacheError::Database(format!("Task execution failed: {e}")))?
    }

    pub fn calculate_hash(
        source_lang: &str,
        target_lang: &str,
        provider: &str,
        text: &str,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(source_lang.as_bytes());
        hasher.update(b"\x00");
        hasher.update(target_lang.as_bytes());
        hasher.update(b"\x00");
        hasher.update(provider.as_bytes());
        hasher.update(b"\x00");
        hasher.update(text.as_bytes());
        let hash = hasher.finalize();
        let mut out = String::with_capacity(hash.len() * 2);
        for &b in hash.as_slice() {
            let _ = std::fmt::write(&mut out, format_args!("{:02x}", b));
        }
        out
    }

    pub fn calculate_block_hash(
        source_lang: &str,
        target_lang: &str,
        provider: &str,
        block_text: &str,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"blk\x00");
        hasher.update(source_lang.as_bytes());
        hasher.update(b"\x00");
        hasher.update(target_lang.as_bytes());
        hasher.update(b"\x00");
        hasher.update(provider.as_bytes());
        hasher.update(b"\x00");
        hasher.update(block_text.as_bytes());
        let hash = hasher.finalize();
        let mut out = String::with_capacity(4 + hash.len() * 2);
        out.push_str("blk:");
        for &b in hash.as_slice() {
            let _ = std::fmt::write(&mut out, format_args!("{:02x}", b));
        }
        out
    }

    async fn find_by_hash(&self, hash: &str) -> Result<Option<String>, CacheError> {
        let conn = Arc::clone(&self.conn);
        let hash = hash.to_string();

        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|e| CacheError::Database(format!("Database lock poisoned: {e}")))?;

            let mut stmt = conn
                .prepare("SELECT translated_text FROM translations WHERE hash = ?1 LIMIT 1")
                .map_err(|e| {
                    CacheError::Database(format!("Failed to prepare select query: {e}"))
                })?;

            let mut rows = stmt
                .query(params![hash])
                .map_err(|e| CacheError::Database(format!("Failed to execute query: {e}")))?;

            if let Some(row) = rows
                .next()
                .map_err(|e| CacheError::Database(format!("Failed to fetch row: {e}")))?
            {
                let val: rusqlite::types::Value = row
                    .get(0)
                    .map_err(|e| CacheError::Database(format!("Failed to read column: {e}")))?;

                match val {
                    rusqlite::types::Value::Blob(bytes) => {
                        if bytes.is_empty() {
                            return Ok(None);
                        }
                        const ZSTD_MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];
                        if bytes.len() >= 4 && bytes[..4] == ZSTD_MAGIC {
                            let decompressed = zstd::decode_all(&bytes[..]).map_err(|e| {
                                CacheError::Serialization(format!(
                                    "Failed to decompress zstd cache payload: {e}"
                                ))
                            })?;
                            let text = String::from_utf8(decompressed).map_err(|e| {
                                CacheError::Serialization(format!(
                                    "Invalid UTF-8 in decompressed cache payload: {e}"
                                ))
                            })?;
                            if text.trim().is_empty() {
                                Ok(None)
                            } else {
                                Ok(Some(text))
                            }
                        } else {
                            let text = String::from_utf8(bytes).map_err(|e| {
                                CacheError::Serialization(format!(
                                    "Invalid UTF-8 in raw cache payload: {e}"
                                ))
                            })?;
                            if text.trim().is_empty() {
                                Ok(None)
                            } else {
                                Ok(Some(text))
                            }
                        }
                    }
                    rusqlite::types::Value::Text(text) => {
                        if text.trim().is_empty() {
                            Ok(None)
                        } else {
                            Ok(Some(text))
                        }
                    }
                    _ => Ok(None),
                }
            } else {
                Ok(None)
            }
        })
        .await
        .map_err(|e| CacheError::Database(format!("Task execution failed: {e}")))?
    }

    async fn insert_translation(
        &self,
        hash: &str,
        source_lang: &str,
        target_lang: &str,
        provider: &str,
        original_text: &str,
        translated_text: &str,
    ) -> Result<(), CacheError> {
        if translated_text.trim().is_empty() {
            return Err(CacheError::Serialization(
                "Refusing to cache empty translated text".to_string(),
            ));
        }

        let conn = Arc::clone(&self.conn);
        let hash = hash.to_string();
        let source_lang = source_lang.to_string();
        let target_lang = target_lang.to_string();
        let provider = provider.to_string();
        let original_text = original_text.to_string();
        let translated_text = translated_text.to_string();

        tokio::task::spawn_blocking(move || {
            let compressed = zstd::encode_all(translated_text.as_bytes(), 3).map_err(|e| {
                CacheError::Serialization(format!("Failed to compress cache payload: {e}"))
            })?;

            let conn = conn
                .lock()
                .map_err(|e| CacheError::Database(format!("Database lock poisoned: {e}")))?;

            conn.execute(
                "INSERT OR REPLACE INTO translations (hash, source_lang, target_lang, provider, original_text, translated_text)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    hash,
                    source_lang,
                    target_lang,
                    provider,
                    original_text,
                    rusqlite::types::Value::Blob(compressed),
                ],
            )
            .map_err(|e| CacheError::Database(format!("Failed to insert into cache: {e}")))?;

            Ok(())
        })
        .await
        .map_err(|e| CacheError::Database(format!("Task execution failed: {e}")))?
    }
}

#[async_trait]
impl TranslationCachePort for SqliteCacheAdapter {
    async fn get(
        &self,
        source_lang: &str,
        target_lang: &str,
        provider: &str,
        text: &str,
    ) -> Result<Option<String>, CacheError> {
        let hash = Self::calculate_hash(source_lang, target_lang, provider, text);
        self.find_by_hash(&hash).await
    }

    async fn set(
        &self,
        source_lang: &str,
        target_lang: &str,
        provider: &str,
        original_text: &str,
        translated_text: &str,
    ) -> Result<(), CacheError> {
        let hash = Self::calculate_hash(source_lang, target_lang, provider, original_text);
        self.insert_translation(
            &hash,
            source_lang,
            target_lang,
            provider,
            original_text,
            translated_text,
        )
        .await
    }

    async fn get_block(
        &self,
        source_lang: &str,
        target_lang: &str,
        provider: &str,
        block_text: &str,
    ) -> Result<Option<String>, CacheError> {
        let hash = Self::calculate_block_hash(source_lang, target_lang, provider, block_text);
        self.find_by_hash(&hash).await
    }

    async fn set_block(
        &self,
        source_lang: &str,
        target_lang: &str,
        provider: &str,
        original_block: &str,
        translated_block: &str,
    ) -> Result<(), CacheError> {
        let hash = Self::calculate_block_hash(source_lang, target_lang, provider, original_block);
        self.insert_translation(
            &hash,
            source_lang,
            target_lang,
            provider,
            original_block,
            translated_block,
        )
        .await
    }

    async fn record_metrics(
        &self,
        provider: &str,
        target_lang: &str,
        metrics: &ExecutionMetrics,
    ) -> Result<(), CacheError> {
        let conn = Arc::clone(&self.conn);
        let provider = provider.to_string();
        let target_lang = target_lang.to_string();
        let total_chapters = metrics.total_chapters as i64;
        let cache_hits = metrics.cache_hits as i64;
        let cache_misses = metrics.cache_misses as i64;
        let hit_ratio = metrics.cache_hit_ratio();
        let avg_latency = metrics.avg_provider_latency().as_secs_f64() * 1000.0;
        let total_duration = metrics.total_elapsed.as_secs_f64();

        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|e| CacheError::Database(format!("Database lock poisoned: {e}")))?;
            conn.execute(
                "INSERT INTO execution_metrics_history (
                    provider, target_lang, total_chapters, cache_hits, cache_misses,
                    hit_ratio_pct, avg_latency_ms, total_duration_secs
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    provider,
                    target_lang,
                    total_chapters,
                    cache_hits,
                    cache_misses,
                    hit_ratio,
                    avg_latency,
                    total_duration,
                ],
            )
            .map_err(|e| {
                CacheError::Database(format!("Failed to insert execution metrics: {e}"))
            })?;

            Ok(())
        })
        .await
        .map_err(|e| CacheError::Database(format!("Task execution failed: {e}")))?
    }

    async fn get_metrics_history(
        &self,
        limit: usize,
    ) -> Result<Vec<ExecutionMetricsRecord>, CacheError> {
        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|e| CacheError::Database(format!("Database lock poisoned: {e}")))?;
            let mut stmt = conn
                .prepare(
                    "SELECT id, timestamp, provider, target_lang, total_chapters, cache_hits, cache_misses,
                            hit_ratio_pct, avg_latency_ms, total_duration_secs
                     FROM execution_metrics_history
                     ORDER BY id DESC
                     LIMIT ?1",
                )
                .map_err(|e| CacheError::Database(format!("Failed to prepare select metrics query: {e}")))?;

            let rows = stmt
                .query_map(params![limit as i64], |row| {
                    Ok(ExecutionMetricsRecord {
                        id: row.get(0)?,
                        timestamp: row.get(1)?,
                        provider: row.get(2)?,
                        target_lang: row.get(3)?,
                        total_chapters: row.get::<_, i64>(4)? as usize,
                        cache_hits: row.get::<_, i64>(5)? as usize,
                        cache_misses: row.get::<_, i64>(6)? as usize,
                        hit_ratio_pct: row.get(7)?,
                        avg_latency_ms: row.get(8)?,
                        total_duration_secs: row.get(9)?,
                    })
                })
                .map_err(|e| CacheError::Database(format!("Failed to execute metrics query: {e}")))?;

            let mut results = Vec::new();
            for row in rows {
                results.push(
                    row.map_err(|e| CacheError::Database(format!("Failed to read metrics row: {e}")))?,
                );
            }
            Ok(results)
        })
        .await
        .map_err(|e| CacheError::Database(format!("Task execution failed: {e}")))?
    }
}
