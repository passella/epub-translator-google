use serde::{Deserialize, Serialize, Serializer};
use std::fmt::{Debug, Display, Formatter, Result as FmtResult};
use std::ops::Deref;
use std::str::FromStr;
use std::time::Duration;
use zeroize::Zeroizing;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Language(String);

impl Language {
    pub fn new(code: impl Into<String>) -> Self {
        Self(code.into())
    }

    pub fn auto() -> Self {
        Self("auto".to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_auto(&self) -> bool {
        self.0.eq_ignore_ascii_case("auto")
    }
}

impl Deref for Language {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for Language {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for Language {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for Language {
    fn from(s: String) -> Self {
        Self(s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TranslationProviderKind {
    Antigravity,
    GoogleCloud,
    GoogleFree,
}

impl TranslationProviderKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Antigravity => "antigravity",
            Self::GoogleCloud => "google-cloud",
            Self::GoogleFree => "google-free",
        }
    }
}

impl Display for TranslationProviderKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for TranslationProviderKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "antigravity" | "agy" | "gemini" => Ok(Self::Antigravity),
            "google-cloud" | "cloud" | "gcp" => Ok(Self::GoogleCloud),
            "google-free" | "free" | "gtx" => Ok(Self::GoogleFree),
            other => Err(format!("Unknown translation provider: {other}")),
        }
    }
}

fn serialize_redacted_api_key<S>(_api_key: &str, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str("[REDACTED]")
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderConfig {
    Antigravity {
        model: Option<String>,
        block_concurrency: Option<usize>,
    },
    GoogleCloud {
        #[serde(serialize_with = "serialize_redacted_api_key")]
        api_key: Zeroizing<String>,
    },
    GoogleFree,
}

impl Debug for ProviderConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Antigravity {
                model,
                block_concurrency,
            } => f
                .debug_struct("Antigravity")
                .field("model", model)
                .field("block_concurrency", block_concurrency)
                .finish(),
            Self::GoogleCloud { .. } => f
                .debug_struct("GoogleCloud")
                .field("api_key", &"[REDACTED]")
                .finish(),
            Self::GoogleFree => f.write_str("GoogleFree"),
        }
    }
}

impl ProviderConfig {
    pub fn antigravity(model: Option<String>) -> Self {
        Self::Antigravity {
            model,
            block_concurrency: None,
        }
    }

    pub fn antigravity_with_block_concurrency(
        model: Option<String>,
        block_concurrency: usize,
    ) -> Self {
        Self::Antigravity {
            model,
            block_concurrency: Some(block_concurrency),
        }
    }

    pub fn google_free() -> Self {
        Self::GoogleFree
    }

    pub fn google_cloud(api_key: impl Into<String>) -> Self {
        Self::GoogleCloud {
            api_key: Zeroizing::new(api_key.into()),
        }
    }

    pub fn new(
        provider_kind: TranslationProviderKind,
        model: Option<String>,
        api_key: Option<String>,
    ) -> Self {
        match provider_kind {
            TranslationProviderKind::Antigravity => Self::Antigravity {
                model,
                block_concurrency: None,
            },
            TranslationProviderKind::GoogleFree => Self::GoogleFree,
            TranslationProviderKind::GoogleCloud => Self::GoogleCloud {
                api_key: Zeroizing::new(api_key.unwrap_or_default()),
            },
        }
    }

    pub fn provider_kind(&self) -> TranslationProviderKind {
        match self {
            Self::Antigravity { .. } => TranslationProviderKind::Antigravity,
            Self::GoogleCloud { .. } => TranslationProviderKind::GoogleCloud,
            Self::GoogleFree => TranslationProviderKind::GoogleFree,
        }
    }

    pub fn model(&self) -> Option<&str> {
        match self {
            Self::Antigravity { model, .. } => model.as_deref(),
            _ => None,
        }
    }

    pub fn block_concurrency(&self) -> Option<usize> {
        match self {
            Self::Antigravity {
                block_concurrency, ..
            } => *block_concurrency,
            _ => None,
        }
    }

    pub fn api_key(&self) -> Option<&str> {
        match self {
            Self::GoogleCloud { api_key } => Some(api_key.as_str()),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationResult {
    pub translated_text: String,
    pub detected_source: Option<Language>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpubMetadata {
    pub title: Option<String>,
    pub language: Option<Language>,
    pub creator: Option<String>,
    pub identifier: Option<String>,
    pub publisher: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpubChapter {
    pub path: String,
    pub content: String,
    pub media_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpubResource {
    pub path: String,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpubBook {
    pub metadata: EpubMetadata,
    pub chapters: Vec<EpubChapter>,
    pub resources: Vec<EpubResource>,
    pub raw_container_xml: Option<String>,
    pub raw_opf_content: Option<String>,
    pub opf_path: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChapterLatency {
    pub index: usize,
    pub path: String,
    pub latency: Duration,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExecutionMetrics {
    pub total_chapters: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub empty_chapters: usize,
    pub chapter_latencies: Vec<ChapterLatency>,
    pub total_elapsed: Duration,
}

impl ExecutionMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cache_hit_ratio(&self) -> f64 {
        let active_total = self.cache_hits + self.cache_misses;
        if active_total == 0 {
            0.0
        } else {
            (self.cache_hits as f64 / active_total as f64) * 100.0
        }
    }

    pub fn avg_provider_latency(&self) -> Duration {
        if self.chapter_latencies.is_empty() {
            Duration::ZERO
        } else {
            let total_micros: u128 = self
                .chapter_latencies
                .iter()
                .map(|cl| cl.latency.as_micros())
                .sum();
            Duration::from_micros((total_micros / self.chapter_latencies.len() as u128) as u64)
        }
    }

    pub fn min_provider_latency(&self) -> Option<Duration> {
        self.chapter_latencies.iter().map(|cl| cl.latency).min()
    }

    pub fn max_provider_latency(&self) -> Option<Duration> {
        self.chapter_latencies.iter().map(|cl| cl.latency).max()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExecutionMetricsRecord {
    pub id: i64,
    pub timestamp: String,
    pub provider: String,
    pub target_lang: String,
    pub total_chapters: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub hit_ratio_pct: f64,
    pub avg_latency_ms: f64,
    pub total_duration_secs: f64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpubVerificationReport {
    pub is_valid: bool,
    pub mimetype_valid: bool,
    pub container_xml_valid: bool,
    pub opf_valid: bool,
    pub total_xhtml_files: usize,
    pub valid_xhtml_files: usize,
    pub errors: Vec<String>,
}
