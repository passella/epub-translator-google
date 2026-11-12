use crate::domain::entities::ProviderConfig;
use crate::domain::errors::AppError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderOptions {
    pub model: Option<String>,
    pub block_concurrency: usize,
    pub api_key: Option<String>,
    pub cloud_key: Option<String>,
    pub is_headless: bool,
}

impl Default for ProviderOptions {
    fn default() -> Self {
        Self {
            model: None,
            block_concurrency: 2,
            api_key: None,
            cloud_key: None,
            is_headless: false,
        }
    }
}

impl ProviderOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_model(mut self, model: Option<String>) -> Self {
        self.model = model;
        self
    }

    pub fn with_block_concurrency(mut self, block_concurrency: usize) -> Self {
        self.block_concurrency = block_concurrency;
        self
    }

    pub fn with_api_key(mut self, api_key: Option<String>) -> Self {
        self.api_key = api_key;
        self
    }

    pub fn with_cloud_key(mut self, cloud_key: Option<String>) -> Self {
        self.cloud_key = cloud_key;
        self
    }

    pub fn with_headless(mut self, is_headless: bool) -> Self {
        self.is_headless = is_headless;
        self
    }
}

pub trait ProviderOptionResolver: Send + Sync {
    fn resolve(&self, options: &ProviderOptions) -> Result<ProviderConfig, AppError>;
}

pub struct AntigravityOptionResolver;

impl ProviderOptionResolver for AntigravityOptionResolver {
    fn resolve(&self, options: &ProviderOptions) -> Result<ProviderConfig, AppError> {
        let model = options.model.as_ref().and_then(|m| {
            let trimmed = m.trim();
            if trimmed.is_empty()
                || trimmed.eq_ignore_ascii_case("default")
                || trimmed.contains("Padrão")
            {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        Ok(ProviderConfig::Antigravity {
            model,
            block_concurrency: Some(options.block_concurrency),
        })
    }
}

pub struct GoogleFreeOptionResolver;

impl ProviderOptionResolver for GoogleFreeOptionResolver {
    fn resolve(&self, _options: &ProviderOptions) -> Result<ProviderConfig, AppError> {
        Ok(ProviderConfig::GoogleFree)
    }
}

pub struct GoogleCloudOptionResolver;

impl GoogleCloudOptionResolver {
    pub fn resolve_api_key(&self, options: &ProviderOptions) -> Result<String, AppError> {
        self.resolve_api_key_with_env(
            options,
            || std::env::var("GOOGLE_TRANSLATE_API_KEY").ok(),
            || std::env::var("GOOGLE_API_KEY").ok(),
        )
    }

    pub fn resolve_api_key_with_env<F1, F2>(
        &self,
        options: &ProviderOptions,
        translate_api_key_fn: F1,
        google_api_key_fn: F2,
    ) -> Result<String, AppError>
    where
        F1: FnOnce() -> Option<String>,
        F2: FnOnce() -> Option<String>,
    {
        if let Some(key) = options
            .cloud_key
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Ok(key.to_string());
        }

        if let Some(key) = options
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Ok(key.to_string());
        }

        if let Some(key) = translate_api_key_fn() {
            let trimmed = key.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }

        if let Some(key) = google_api_key_fn() {
            let trimmed = key.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }

        Ok(String::new())
    }
}

impl ProviderOptionResolver for GoogleCloudOptionResolver {
    fn resolve(&self, options: &ProviderOptions) -> Result<ProviderConfig, AppError> {
        let api_key = self.resolve_api_key(options)?;
        Ok(ProviderConfig::GoogleCloud { api_key })
    }
}
