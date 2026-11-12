pub mod antigravity;
pub mod common;
pub mod factory;
pub mod google_cloud;
pub mod google_free;
pub mod http_util;
pub mod options;

pub use antigravity::{
    AntigravityTranslatorAdapter, RetryConfig, calculate_backoff, is_transient_error,
};
pub use common::translate_batch_sequential;
pub use factory::{
    ProviderMetadata, ProviderRegistration, ProviderRegistry, TranslatorConstructor,
    TranslatorFactory,
};
pub use google_cloud::GoogleCloudTranslatorAdapter;
pub use google_free::GoogleFreeTranslatorAdapter;
pub use http_util::{
    HttpRequestSnapshot, extract_headers, extract_headers_redacted, extract_request_body,
};
pub use options::{
    AntigravityOptionResolver, GoogleCloudOptionResolver, GoogleFreeOptionResolver,
    ProviderOptionResolver, ProviderOptions,
};
