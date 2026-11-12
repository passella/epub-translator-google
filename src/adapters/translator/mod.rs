pub mod antigravity;
pub mod factory;
pub mod google_cloud;
pub mod google_free;
pub mod options;

pub use antigravity::{
    AntigravityTranslatorAdapter, RetryConfig, calculate_backoff, is_transient_error,
};
pub use factory::{
    ProviderMetadata, ProviderRegistration, ProviderRegistry, TranslatorConstructor,
    TranslatorFactory,
};
pub use google_cloud::GoogleCloudTranslatorAdapter;
pub use google_free::GoogleFreeTranslatorAdapter;
pub use options::{
    AntigravityOptionResolver, GoogleCloudOptionResolver, GoogleFreeOptionResolver,
    ProviderOptionResolver, ProviderOptions,
};
