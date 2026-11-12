use crate::domain::entities::{Language, TranslationResult};
use crate::domain::errors::{ProcessErrorDetails, TranslationError};
use crate::domain::ports::{ModelDiscoveryPort, TranslationCachePort, TranslatorPort};
use crate::domain::services::{XhtmlPartitioner, XhtmlValidator};
use async_trait::async_trait;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::sleep;

static JITTER_SEED: AtomicU64 = AtomicU64::new(123456789);

#[derive(Clone, Debug, PartialEq)]
pub struct RetryConfig {
    pub max_attempts: usize,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(1000),
            max_delay: Duration::from_millis(8000),
            backoff_multiplier: 2.0,
        }
    }
}

pub fn is_transient_error(stdout: &str, stderr: &str) -> bool {
    let combined = format!("{stdout}\n{stderr}").to_lowercase();
    combined.contains("500 internal server error")
        || combined.contains("internal server error")
        || combined.contains("eligibility check failed")
        || combined.contains("network")
        || combined.contains("connection reset")
        || combined.contains("connection refused")
        || combined.contains("timed out")
        || combined.contains("timeout")
        || combined.contains("rate limit")
        || combined.contains("too many requests")
        || combined.contains("429")
        || combined.contains("502 bad gateway")
        || combined.contains("bad gateway")
        || combined.contains("503 service unavailable")
        || combined.contains("service unavailable")
        || combined.contains("504 gateway timeout")
        || combined.contains("gateway timeout")
        || combined.contains("temporary failure")
        || combined.contains("socket")
        || combined.contains("broken pipe")
        || combined.contains("econnreset")
        || combined.contains("etimedout")
}

pub fn calculate_backoff(
    attempt: usize,
    initial_delay: Duration,
    max_delay: Duration,
    multiplier: f64,
) -> Duration {
    let factor = multiplier.powi(attempt as i32);
    let base_ms = (initial_delay.as_millis() as f64 * factor) as u64;
    let capped_ms = base_ms.min(max_delay.as_millis() as u64);
    let seed = JITTER_SEED.fetch_add(1103515245, Ordering::Relaxed);
    let jitter_range = (capped_ms / 4).min(500);
    let jitter_ms = if jitter_range > 0 {
        seed % (jitter_range + 1)
    } else {
        0
    };
    Duration::from_millis(capped_ms + jitter_ms)
}

#[derive(Deserialize)]
struct AgyJsonResponse {
    response: Option<String>,
    status: Option<String>,
}

#[derive(Clone)]
pub struct AntigravityTranslatorAdapter {
    binary_path: PathBuf,
    model: Option<String>,
    retry_config: RetryConfig,
    partition_threshold: usize,
    cache: Option<Arc<dyn TranslationCachePort>>,
    block_concurrency: usize,
}

impl AntigravityTranslatorAdapter {
    pub const DEFAULT_BLOCK_CONCURRENCY: usize = 2;

    pub fn new(model: Option<String>) -> Self {
        let binary_path = Self::find_binary_path();
        Self {
            binary_path,
            model,
            retry_config: RetryConfig::default(),
            partition_threshold: XhtmlPartitioner::DEFAULT_THRESHOLD,
            cache: None,
            block_concurrency: Self::DEFAULT_BLOCK_CONCURRENCY,
        }
    }

    pub fn with_retry_config(mut self, retry_config: RetryConfig) -> Self {
        self.retry_config = retry_config;
        self
    }

    pub fn retry_config(&self) -> &RetryConfig {
        &self.retry_config
    }

    pub fn with_partition_threshold(mut self, threshold: usize) -> Self {
        self.partition_threshold = threshold.max(1);
        self
    }

    pub fn partition_threshold(&self) -> usize {
        self.partition_threshold
    }

    pub fn with_cache(mut self, cache: Arc<dyn TranslationCachePort>) -> Self {
        self.cache = Some(cache);
        self
    }

    pub fn cache(&self) -> Option<&Arc<dyn TranslationCachePort>> {
        self.cache.as_ref()
    }

    pub fn with_block_concurrency(mut self, concurrency: usize) -> Self {
        self.block_concurrency = concurrency.max(1);
        self
    }

    pub fn block_concurrency(&self) -> usize {
        self.block_concurrency
    }

    pub fn with_binary_path(path: impl AsRef<Path>, model: Option<String>) -> Self {
        Self {
            binary_path: path.as_ref().to_path_buf(),
            model,
            retry_config: RetryConfig::default(),
            partition_threshold: XhtmlPartitioner::DEFAULT_THRESHOLD,
            cache: None,
            block_concurrency: Self::DEFAULT_BLOCK_CONCURRENCY,
        }
    }

    pub fn find_binary_path() -> PathBuf {
        if let Ok(home_var) = std::env::var("HOME") {
            let local_bin = PathBuf::from(home_var).join(".local/bin/agy");
            if local_bin.is_file() {
                return local_bin;
            }
        }
        if let Some(home_buf) = dirs::home_dir() {
            let local_bin = home_buf.join(".local/bin/agy");
            if local_bin.is_file() {
                return local_bin;
            }
        }
        if let Some(paths) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&paths) {
                let candidate = dir.join("agy");
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
        PathBuf::from("agy")
    }

    pub fn resolve_binary_path(home: Option<&str>) -> PathBuf {
        Self::resolve_binary_path_with_env(home, None)
    }

    pub fn resolve_binary_path_with_env(
        home: Option<&str>,
        path_env: Option<&std::ffi::OsStr>,
    ) -> PathBuf {
        if let Some(home_path) = home {
            let local_bin = PathBuf::from(home_path).join(".local/bin/agy");
            if local_bin.is_file() {
                return local_bin;
            }
        }

        if let Some(paths) = path_env {
            for dir in std::env::split_paths(paths) {
                let candidate = dir.join("agy");
                if candidate.is_file() {
                    return candidate;
                }
            }
        }

        PathBuf::from("agy")
    }

    pub async fn fetch_available_models(binary_path: &Path) -> Vec<(String, String)> {
        let output = match tokio::process::Command::new(binary_path)
            .arg("models")
            .stderr(std::process::Stdio::null())
            .output()
            .await
        {
            Ok(out) => out,
            Err(_) => return Vec::new(),
        };

        if !output.status.success() {
            return Vec::new();
        }

        let text = String::from_utf8_lossy(&output.stdout);
        let mut models = Vec::new();
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }
            let id = parts[0].to_string();
            let desc = if parts.len() > 1 {
                parts[1..].join(" ")
            } else {
                id.clone()
            };
            models.push((id, desc));
        }
        models
    }

    fn strip_code_fences(content: &str) -> String {
        let trimmed = content.trim();
        let prefixes = ["```html", "```xhtml", "```xml", "```"];
        for prefix in prefixes {
            if let Some(stripped) = trimmed.strip_prefix(prefix)
                && let Some(inner) = stripped.strip_suffix("```")
            {
                return inner.trim().to_string();
            }
        }
        trimmed.to_string()
    }

    async fn execute_single_attempt(
        &self,
        prompt: &str,
        cmd_args: &[String],
    ) -> Result<TranslationResult, (TranslationError, bool)> {
        let mut cmd = Command::new(&self.binary_path);
        for arg in cmd_args {
            cmd.arg(arg);
        }

        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            let is_transient = matches!(
                e.kind(),
                std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::TimedOut
                    | std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::Interrupted
            ) || e.raw_os_error() == Some(26);
            (
                TranslationError::ProcessDetailed(Box::new(ProcessErrorDetails {
                    command: self.binary_path.display().to_string(),
                    args: cmd_args.to_vec(),
                    io_mode: "stdin (streaming pipe)".to_string(),
                    exit_code: None,
                    os_error_code: e.raw_os_error(),
                    stdout: String::new(),
                    stderr: String::new(),
                    message: format!("Failed to spawn agy CLI: {e}"),
                    location: crate::here!(),
                })),
                is_transient,
            )
        })?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(prompt.as_bytes()).await.map_err(|e| {
                let is_transient = matches!(
                    e.kind(),
                    std::io::ErrorKind::BrokenPipe
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::TimedOut
                );
                (
                    TranslationError::ProcessDetailed(Box::new(ProcessErrorDetails {
                        command: self.binary_path.display().to_string(),
                        args: cmd_args.to_vec(),
                        io_mode: "stdin (streaming pipe)".to_string(),
                        exit_code: None,
                        os_error_code: e.raw_os_error(),
                        stdout: String::new(),
                        stderr: String::new(),
                        message: format!("Failed to write prompt to agy stdin: {e}"),
                        location: crate::here!(),
                    })),
                    is_transient,
                )
            })?;
        }

        let output = child.wait_with_output().await.map_err(|e| {
            (
                TranslationError::ProcessDetailed(Box::new(ProcessErrorDetails {
                    command: self.binary_path.display().to_string(),
                    args: cmd_args.to_vec(),
                    io_mode: "stdin (streaming pipe)".to_string(),
                    exit_code: None,
                    os_error_code: e.raw_os_error(),
                    stdout: String::new(),
                    stderr: String::new(),
                    message: format!("Failed to wait for agy CLI: {e}"),
                    location: crate::here!(),
                })),
                false,
            )
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            let transient = is_transient_error(&stdout, &stderr);
            return Err((
                TranslationError::ProcessDetailed(Box::new(ProcessErrorDetails {
                    command: self.binary_path.display().to_string(),
                    args: cmd_args.to_vec(),
                    io_mode: "stdin (streaming pipe)".to_string(),
                    exit_code: output.status.code(),
                    os_error_code: None,
                    stdout,
                    stderr: stderr.clone(),
                    message: format!("agy CLI failed with status {}: {stderr}", output.status),
                    location: crate::here!(),
                })),
                transient,
            ));
        }

        let parsed: AgyJsonResponse = match serde_json::from_str(&stdout) {
            Ok(val) => val,
            Err(e) => {
                let transient = is_transient_error(&stdout, &stderr);
                return Err((
                    TranslationError::Serialization(format!(
                        "Failed to deserialize agy JSON output: {e}. Raw stdout: {stdout}"
                    )),
                    transient,
                ));
            }
        };

        let response_text = match parsed.response {
            Some(resp) => resp,
            None => {
                let status_msg = parsed.status.unwrap_or_default();
                let transient = is_transient_error(&status_msg, &stderr);
                return Err((
                    TranslationError::Provider(format!(
                        "agy JSON response missing response field. Status: {status_msg}"
                    )),
                    transient,
                ));
            }
        };

        let cleaned_html = Self::strip_code_fences(&response_text);
        if cleaned_html.trim().is_empty() {
            return Err((
                TranslationError::Provider("agy returned empty translation content".to_string()),
                true,
            ));
        }

        Ok(TranslationResult {
            translated_text: cleaned_html,
            detected_source: None,
        })
    }

    async fn translate_single_content(
        &self,
        content: &str,
        source_lang: &Language,
        target_lang: &Language,
    ) -> Result<TranslationResult, TranslationError> {
        let prompt = format!(
            "You are a professional literary book translator. Translate the following XHTML/HTML document content from source language '{}' into target language '{}'. \
            STRICT INSTRUCTIONS:\n\
            1. Strictly preserve all HTML/XML tags, attributes, formatting, CSS classes, IDs and document hierarchy completely intact.\n\
            2. Only translate human-readable text contents inside the tags.\n\
            3. Do NOT include markdown code fences (```), greetings, or explanations.\n\
            4. Output ONLY the raw translated XHTML/HTML document content.\n\n\
            DOCUMENT CONTENT TO TRANSLATE:\n{}",
            source_lang.as_str(),
            target_lang.as_str(),
            content
        );

        let mut cmd_args = vec![
            "--output-format".to_string(),
            "json".to_string(),
            "--disable-slash-commands".to_string(),
        ];

        if let Some(ref model) = self.model {
            cmd_args.push("--model".to_string());
            cmd_args.push(model.clone());
        } else {
            cmd_args.push("--effort".to_string());
            cmd_args.push("low".to_string());
        }

        let max_attempts = self.retry_config.max_attempts.max(1);

        for attempt in 0..max_attempts {
            match self.execute_single_attempt(&prompt, &cmd_args).await {
                Ok(result) => return Ok(result),
                Err((err, is_transient)) => {
                    let has_next_attempt = attempt + 1 < max_attempts;
                    if is_transient && has_next_attempt {
                        let delay = calculate_backoff(
                            attempt,
                            self.retry_config.initial_delay,
                            self.retry_config.max_delay,
                            self.retry_config.backoff_multiplier,
                        );
                        sleep(delay).await;
                        continue;
                    }
                    return Err(err);
                }
            }
        }

        Err(TranslationError::Process(
            "Exhausted all retry attempts without success".to_string(),
        ))
    }
}

impl Default for AntigravityTranslatorAdapter {
    fn default() -> Self {
        Self::new(None)
    }
}

#[async_trait]
impl TranslatorPort for AntigravityTranslatorAdapter {
    async fn translate_html(
        &self,
        html: &str,
        source_lang: &Language,
        target_lang: &Language,
    ) -> Result<TranslationResult, TranslationError> {
        let partitioner = XhtmlPartitioner::with_threshold(self.partition_threshold);
        if !partitioner.is_giant(html) {
            let result = self
                .translate_single_content(html, source_lang, target_lang)
                .await?;
            let validated =
                XhtmlValidator::heal_and_validate(&result.translated_text).map_err(|e| {
                    TranslationError::XmlValidation(format!(
                        "agy translation output failed XML 1.0 validation: {e}"
                    ))
                })?;
            return Ok(TranslationResult {
                translated_text: validated,
                detected_source: result.detected_source,
            });
        }

        let plan = partitioner.partition(html);
        let translatable_chunks = plan.translatable_chunks();

        if translatable_chunks.is_empty() {
            return Ok(TranslationResult {
                translated_text: html.to_string(),
                detected_source: None,
            });
        }

        let total_chunks = translatable_chunks.len();
        let mut translated_pieces: Vec<Option<String>> = vec![None; total_chunks];
        let mut uncached_indices = Vec::new();

        if let Some(cache) = &self.cache {
            for (idx, chunk) in translatable_chunks.iter().enumerate() {
                let cached = cache
                    .get_block(
                        source_lang.as_str(),
                        target_lang.as_str(),
                        self.provider_name(),
                        chunk,
                    )
                    .await
                    .map_err(|e| TranslationError::Cache(e.to_string()))?;

                if let Some(text) = cached {
                    translated_pieces[idx] = Some(text);
                } else {
                    uncached_indices.push(idx);
                }
            }
        } else {
            for idx in 0..total_chunks {
                uncached_indices.push(idx);
            }
        }

        if uncached_indices.is_empty() {
            let final_pieces: Vec<String> = translated_pieces.into_iter().flatten().collect();
            let recomposed = plan.recompose(&final_pieces);
            let validated = XhtmlValidator::heal_and_validate(&recomposed).map_err(|e| {
                TranslationError::XmlValidation(format!(
                    "agy recomposed translation output failed XML 1.0 validation: {e}"
                ))
            })?;
            return Ok(TranslationResult {
                translated_text: validated,
                detected_source: None,
            });
        }

        let semaphore = Arc::new(Semaphore::new(self.block_concurrency));
        let mut join_set = JoinSet::new();
        let adapter = Arc::new(self.clone());

        for idx in uncached_indices {
            let chunk = translatable_chunks[idx].to_string();
            let sem = semaphore.clone();
            let adapter = adapter.clone();
            let src = source_lang.clone();
            let tgt = target_lang.clone();

            join_set.spawn(async move {
                let _permit = sem.acquire().await.map_err(|e| {
                    TranslationError::Process(format!("Semaphore permit acquire failed: {e}"))
                })?;
                let result = adapter.translate_single_content(&chunk, &src, &tgt).await?;
                Ok((idx, chunk, result))
            });
        }

        let mut detected_source = None;
        let provider_name = self.provider_name();
        let cache_opt = self.cache.clone();

        while let Some(join_res) = join_set.join_next().await {
            match join_res {
                Ok(Ok((idx, original_chunk, result))) => {
                    if detected_source.is_none() && result.detected_source.is_some() {
                        detected_source = result.detected_source.clone();
                    }

                    if let Some(cache) = &cache_opt {
                        cache
                            .set_block(
                                source_lang.as_str(),
                                target_lang.as_str(),
                                provider_name,
                                &original_chunk,
                                &result.translated_text,
                            )
                            .await
                            .map_err(|e| TranslationError::Cache(e.to_string()))?;

                        if source_lang.is_auto()
                            && let Some(ref detected) = result.detected_source
                        {
                            let _ = cache
                                .set_block(
                                    detected.as_str(),
                                    target_lang.as_str(),
                                    provider_name,
                                    &original_chunk,
                                    &result.translated_text,
                                )
                                .await;
                        }
                    }

                    translated_pieces[idx] = Some(result.translated_text);
                }
                Ok(Err(trans_err)) => {
                    join_set.abort_all();
                    return Err(trans_err);
                }
                Err(join_err) => {
                    join_set.abort_all();
                    return Err(TranslationError::Process(format!(
                        "Task join error during block translation: {join_err}"
                    )));
                }
            }
        }

        let mut final_pieces = Vec::with_capacity(total_chunks);
        for (i, piece_opt) in translated_pieces.into_iter().enumerate() {
            let piece = piece_opt.ok_or_else(|| {
                TranslationError::Process(format!("Missing translated piece at index {i}"))
            })?;
            final_pieces.push(piece);
        }

        let recomposed = plan.recompose(&final_pieces);
        let validated = XhtmlValidator::heal_and_validate(&recomposed).map_err(|e| {
            TranslationError::XmlValidation(format!(
                "agy recomposed translation output failed XML 1.0 validation: {e}"
            ))
        })?;
        Ok(TranslationResult {
            translated_text: validated,
            detected_source,
        })
    }

    async fn translate_batch(
        &self,
        texts: &[String],
        source_lang: &Language,
        target_lang: &Language,
    ) -> Result<Vec<TranslationResult>, TranslationError> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            let res = self.translate_html(text, source_lang, target_lang).await?;
            results.push(res);
        }
        Ok(results)
    }

    fn provider_name(&self) -> &'static str {
        "antigravity"
    }

    fn model_name(&self) -> Option<&str> {
        self.model.as_deref()
    }
}

#[async_trait]
impl ModelDiscoveryPort for AntigravityTranslatorAdapter {
    async fn fetch_available_models(&self) -> Vec<(String, String)> {
        Self::fetch_available_models(&self.binary_path).await
    }
}
