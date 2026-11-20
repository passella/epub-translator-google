use crate::adapters::translator::http_util::{HttpRequestSnapshot, extract_headers};
use crate::domain::entities::{Language, TranslationResult};
use crate::domain::errors::TranslationError;
use crate::domain::ports::TranslatorPort;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::time::sleep;

static JITTER_SEED: AtomicU64 = AtomicU64::new(987654321);
const MAX_CHUNK_SIZE: usize = 1800;
const MIN_CHUNK_SPLIT_THRESHOLD: usize = 400;

fn next_jitter_duration() -> Duration {
    let val = JITTER_SEED.fetch_add(1103515245, Ordering::Relaxed);
    let ms = 500 + (val % 501);
    Duration::from_millis(ms)
}

fn is_outside_tag_placeholder(slice: &str) -> bool {
    let last_open = slice.rfind('⟦');
    let last_close = slice.rfind('⟧');
    match (last_open, last_close) {
        (None, _) => true,
        (Some(open), Some(close)) => close > open,
        (Some(_), None) => false,
    }
}

fn find_best_cut_in_window(window: &str, min_threshold: usize) -> Option<usize> {
    if let Some(pos) = window.rfind("\n\n") {
        let cut = pos + 2;
        if cut >= min_threshold && is_outside_tag_placeholder(&window[..cut]) {
            return Some(cut);
        }
    }

    if let Some(pos) = window.rfind('\n') {
        let cut = pos + 1;
        if cut >= min_threshold && is_outside_tag_placeholder(&window[..cut]) {
            return Some(cut);
        }
    }

    if let Some(pos) = window.rfind('⟧') {
        let cut = pos + '⟧'.len_utf8();
        if cut >= min_threshold {
            return Some(cut);
        }
    }

    let mut best_punct = None;
    for punct in &[". ", "? ", "! ", "; "] {
        if let Some(pos) = window.rfind(punct) {
            let cut = pos + punct.len();
            if cut >= min_threshold
                && is_outside_tag_placeholder(&window[..cut])
                && best_punct.is_none_or(|b| cut > b)
            {
                best_punct = Some(cut);
            }
        }
    }
    if let Some(cut) = best_punct {
        return Some(cut);
    }

    if let Some(pos) = window.rfind(' ') {
        let cut = pos + 1;
        if cut >= min_threshold && is_outside_tag_placeholder(&window[..cut]) {
            return Some(cut);
        }
    }

    None
}

pub fn split_into_safe_chunks(text: &str, max_chunk_len: usize) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }

    if text.len() <= max_chunk_len {
        return vec![text];
    }

    let mut chunks = Vec::new();
    let mut current_offset = 0;

    while current_offset < text.len() {
        let remaining_len = text.len() - current_offset;
        if remaining_len <= max_chunk_len {
            chunks.push(&text[current_offset..]);
            break;
        }

        let mut candidate_end = current_offset + max_chunk_len;
        while !text.is_char_boundary(candidate_end) {
            candidate_end -= 1;
        }

        let window = &text[current_offset..candidate_end];
        let mut split_idx = None;

        if let Some(rel_cut) = find_best_cut_in_window(window, MIN_CHUNK_SPLIT_THRESHOLD) {
            split_idx = Some(current_offset + rel_cut);
        } else if let Some(rel_cut) = find_best_cut_in_window(window, 1) {
            split_idx = Some(current_offset + rel_cut);
        } else if !is_outside_tag_placeholder(window)
            && let Some(open_idx) = window.rfind('⟦')
        {
            if open_idx > 0 {
                split_idx = Some(current_offset + open_idx);
            } else if let Some(close_idx) = text[candidate_end..].find('⟧') {
                split_idx = Some(candidate_end + close_idx + '⟧'.len_utf8());
            }
        }

        let mut next_offset = split_idx.unwrap_or(candidate_end);
        if next_offset <= current_offset {
            next_offset = candidate_end.max(current_offset + 1);
            while next_offset < text.len() && !text.is_char_boundary(next_offset) {
                next_offset += 1;
            }
        }

        chunks.push(&text[current_offset..next_offset]);
        current_offset = next_offset;
    }

    chunks
}

pub struct GoogleFreeTranslatorAdapter {
    client: Client,
    semaphore: Arc<Semaphore>,
}

impl GoogleFreeTranslatorAdapter {
    pub fn new() -> Result<Self, TranslationError> {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/133.0.0.0 Safari/537.36")
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .build()
            .map_err(|e| TranslationError::Http(format!("Failed to build reqwest client: {e}")))?;

        Ok(Self {
            client,
            semaphore: Arc::new(Semaphore::new(1)),
        })
    }

    async fn translate_single_text(
        &self,
        text: &str,
        source_lang: &Language,
        target_lang: &Language,
    ) -> Result<TranslationResult, TranslationError> {
        let _permit = self.semaphore.acquire().await.map_err(|e| {
            TranslationError::Process(format!("Failed to acquire semaphore permit: {e}"))
        })?;

        sleep(next_jitter_duration()).await;

        let source_param = if source_lang.is_auto() {
            "auto"
        } else {
            source_lang.as_str()
        };

        let target_param = target_lang.as_str();

        let url = "https://translate.googleapis.com/translate_a/single";
        let params = [
            ("client", "gtx"),
            ("sl", source_param),
            ("tl", target_param),
            ("dt", "t"),
            ("q", text),
        ];

        let request =
            self.client.post(url).form(&params).build().map_err(|e| {
                TranslationError::Http(format!("Failed to build HTTP request: {e}"))
            })?;

        let snapshot = HttpRequestSnapshot::from_request(&request, &[]);
        let response = self.client.execute(request).await.map_err(|e| {
            TranslationError::HttpDetailed(Box::new(
                snapshot.to_network_error_details(format!("Google Free HTTP network failure: {e}")),
            ))
        })?;

        let status = response.status();
        let status_code = status.as_u16();

        if !status.is_success() {
            let response_headers = extract_headers(response.headers());
            let response_body = response
                .text()
                .await
                .unwrap_or_else(|e| format!("[Failed to read body: {e}]"));

            let error_details = Box::new(snapshot.to_http_error_details(
                status_code,
                response_headers,
                response_body.clone(),
                format!("Google Free endpoint failed with HTTP {status}"),
            ));

            if status_code == 429 {
                return Err(TranslationError::RateLimited(format!(
                    "Google Free endpoint returned HTTP 429 (Too Many Requests). O WAF/CAPTCHA do Google impôs restrição temporária por IP devido a requisições consecutivas de chunks. Sugestão: aguarde alguns minutos ou utilize o provedor '--provider antigravity' (que opera com autenticação direta via CLI sem bloqueios por IP web). Resposta: {response_body}"
                )));
            }

            return Err(TranslationError::HttpDetailed(error_details));
        }

        let body: Value = response.json().await.map_err(|e| {
            TranslationError::Serialization(format!(
                "Failed to deserialize Google Free JSON response: {e}"
            ))
        })?;

        let segments = body.get(0).and_then(Value::as_array).ok_or_else(|| {
            TranslationError::Serialization("Missing segments array in response".to_string())
        })?;

        let mut translated_full = String::new();
        for seg in segments {
            if let Some(seg_arr) = seg.as_array()
                && let Some(translated_chunk) = seg_arr.first().and_then(Value::as_str)
            {
                translated_full.push_str(translated_chunk);
            }
        }

        let detected_source = body.get(2).and_then(Value::as_str).map(Language::new);

        Ok(TranslationResult {
            translated_text: translated_full,
            detected_source,
        })
    }

    pub fn mask_and_translate_html_block(&self, block_content: &str) -> (String, Vec<String>) {
        let mut masked = String::with_capacity(block_content.len());
        let mut tags = Vec::new();
        let mut chars = block_content.char_indices().peekable();

        while let Some((idx, ch)) = chars.next() {
            if ch == '<' {
                let start = idx;
                let mut end = None;
                for (c_idx, c_ch) in chars.by_ref() {
                    if c_ch == '>' {
                        end = Some(c_idx + 1);
                        break;
                    }
                }
                if let Some(end_idx) = end {
                    let tag = &block_content[start..end_idx];
                    let placeholder = format!("⟦{}⟧", tags.len());
                    tags.push(tag.to_string());
                    masked.push_str(&placeholder);
                } else {
                    masked.push('<');
                }
            } else {
                masked.push(ch);
            }
        }

        (masked, tags)
    }

    pub fn unmask_html(masked_translated: &str, tags: &[String]) -> String {
        let mut unmasked = masked_translated.to_string();
        for (i, tag) in tags.iter().enumerate().rev() {
            let placeholder = format!("⟦{i}⟧");
            let placeholder_spaced_left = format!("⟦ {i}⟧");
            let placeholder_spaced_right = format!("⟦{i} ⟧");
            let placeholder_spaced_both = format!("⟦ {i} ⟧");

            unmasked = unmasked.replace(&placeholder_spaced_both, tag);
            unmasked = unmasked.replace(&placeholder_spaced_left, tag);
            unmasked = unmasked.replace(&placeholder_spaced_right, tag);
            unmasked = unmasked.replace(&placeholder, tag);
        }
        unmasked
    }

    pub fn normalize_placeholders(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;

        while !rest.is_empty() {
            if rest.starts_with("[[") {
                if let Some(end) = rest[2..].find("]]") {
                    let candidate = &rest[2..2 + end];
                    let trimmed = candidate.trim();
                    if !trimmed.is_empty() && trimmed.chars().all(|c| c.is_ascii_digit()) {
                        out.push('⟦');
                        out.push_str(trimmed);
                        out.push('⟧');
                        rest = &rest[2 + end + 2..];
                        continue;
                    }
                }
            } else if rest.starts_with('⟦') {
                let start_len = '⟦'.len_utf8();
                if let Some(end) = rest[start_len..].find('⟧') {
                    let candidate = &rest[start_len..start_len + end];
                    let trimmed = candidate.trim();
                    if !trimmed.is_empty() && trimmed.chars().all(|c| c.is_ascii_digit()) {
                        out.push('⟦');
                        out.push_str(trimmed);
                        out.push('⟧');
                        rest = &rest[start_len + end + '⟧'.len_utf8()..];
                        continue;
                    }
                }
            } else if rest.starts_with('[')
                && let Some(end) = rest[1..].find(']')
            {
                let candidate = &rest[1..1 + end];
                let trimmed = candidate.trim();
                if !trimmed.is_empty() && trimmed.chars().all(|c| c.is_ascii_digit()) {
                    out.push('⟦');
                    out.push_str(trimmed);
                    out.push('⟧');
                    rest = &rest[1 + end + 1..];
                    continue;
                }
            }

            if let Some(ch) = rest.chars().next() {
                out.push(ch);
                rest = &rest[ch.len_utf8()..];
            }
        }

        out
    }

    pub fn find_orphan_placeholders(text: &str) -> Vec<String> {
        let mut orphans = Vec::new();
        let mut rest = text;
        while let Some(start) = rest.find('⟦') {
            let after_start = &rest[start + '⟦'.len_utf8()..];
            if let Some(end) = after_start.find('⟧') {
                let candidate = &after_start[..end];
                let trimmed = candidate.trim();
                if !trimmed.is_empty() && trimmed.chars().all(|c| c.is_ascii_digit()) {
                    orphans.push(format!("⟦{candidate}⟧"));
                }
                rest = &after_start[end + '⟧'.len_utf8()..];
            } else {
                break;
            }
        }
        orphans
    }

    pub fn has_orphan_placeholders(text: &str) -> bool {
        let mut rest = text;
        while let Some(start) = rest.find('⟦') {
            let after_start = &rest[start + '⟦'.len_utf8()..];
            if let Some(end) = after_start.find('⟧') {
                let candidate = &after_start[..end];
                let trimmed = candidate.trim();
                if !trimmed.is_empty() && trimmed.chars().all(|c| c.is_ascii_digit()) {
                    return true;
                }
                rest = &after_start[end + '⟧'.len_utf8()..];
            } else {
                break;
            }
        }
        false
    }
}

impl Default for GoogleFreeTranslatorAdapter {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            client: Client::new(),
            semaphore: Arc::new(Semaphore::new(1)),
        })
    }
}

#[async_trait]
impl TranslatorPort for GoogleFreeTranslatorAdapter {
    async fn translate_html(
        &self,
        html: &str,
        source_lang: &Language,
        target_lang: &Language,
    ) -> Result<TranslationResult, TranslationError> {
        let (masked, tags) = self.mask_and_translate_html_block(html);

        if masked.trim().is_empty() {
            return Ok(TranslationResult {
                translated_text: html.to_string(),
                detected_source: None,
            });
        }

        let chunks = split_into_safe_chunks(&masked, MAX_CHUNK_SIZE);
        let mut translated_pieces = Vec::with_capacity(chunks.len());
        let mut detected_lang = None;

        for chunk in chunks {
            if chunk.trim().is_empty() {
                translated_pieces.push(chunk.to_string());
                continue;
            }

            let piece_res = self
                .translate_single_text(chunk, source_lang, target_lang)
                .await?;

            if detected_lang.is_none() && piece_res.detected_source.is_some() {
                detected_lang = piece_res.detected_source;
            }

            translated_pieces.push(piece_res.translated_text);
        }

        let combined = translated_pieces.concat();
        let normalized = Self::normalize_placeholders(&combined);
        let unmasked = Self::unmask_html(&normalized, &tags);
        let normalized_unmasked = Self::normalize_placeholders(&unmasked);

        let orphans = Self::find_orphan_placeholders(&normalized_unmasked);
        if !orphans.is_empty() {
            return Err(TranslationError::Provider(format!(
                "Orphan or corrupted tag placeholders detected after unmasking: {orphans:?}"
            )));
        }

        Ok(TranslationResult {
            translated_text: unmasked,
            detected_source: detected_lang,
        })
    }

    async fn translate_batch(
        &self,
        texts: &[String],
        source_lang: &Language,
        target_lang: &Language,
    ) -> Result<Vec<TranslationResult>, TranslationError> {
        super::common::translate_batch_sequential(self, texts, source_lang, target_lang).await
    }

    fn provider_name(&self) -> &'static str {
        "google-free"
    }
}
