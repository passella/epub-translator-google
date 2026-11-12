use clap::Parser;
use epub_translator_google::adapters::cli::args::CliArgs;
use epub_translator_google::adapters::cli::provider_resolver::{
    AntigravityOptionResolver, GoogleCloudOptionResolver, GoogleFreeOptionResolver,
    ProviderConfigResolver, ProviderOptionResolver,
};
use epub_translator_google::domain::entities::{ProviderConfig, TranslationProviderKind};

fn create_test_args(model: Option<String>, api_key: Option<String>) -> CliArgs {
    CliArgs {
        model,
        api_key,
        ..Default::default()
    }
}

#[test]
fn test_cli_args_parsing_cloud_key_flag() {
    let args = CliArgs::try_parse_from(["app", "--cloud-key", "my-cloud-key"]).unwrap();
    assert_eq!(args.cloud_key.as_deref(), Some("my-cloud-key"));
    assert_eq!(args.api_key, None);
}

#[test]
fn test_cli_args_parsing_cloud_api_key_alias_flag() {
    let args = CliArgs::try_parse_from(["app", "--cloud-api-key", "my-alias-key"]).unwrap();
    assert_eq!(args.cloud_key.as_deref(), Some("my-alias-key"));
    assert_eq!(args.api_key, None);
}

#[test]
fn test_cli_args_parsing_generic_api_key_flag() {
    let args = CliArgs::try_parse_from(["app", "--api-key", "my-generic-key"]).unwrap();
    assert_eq!(args.api_key.as_deref(), Some("my-generic-key"));
    assert_eq!(args.cloud_key, None);
}

#[test]
fn test_cli_args_parsing_target_lang_alias_flag() {
    let args = CliArgs::try_parse_from(["app", "--target-lang", "pt-br"]).unwrap();
    assert_eq!(args.target.as_deref(), Some("pt-br"));
}

#[test]
fn test_cli_args_parsing_source_lang_alias_flag() {
    let args = CliArgs::try_parse_from(["app", "--source-lang", "en"]).unwrap();
    assert_eq!(args.source.as_deref(), Some("en"));
}

#[test]
fn test_google_cloud_precedence_cloud_key_beats_all() {
    let resolver = GoogleCloudOptionResolver;
    let args = CliArgs {
        cloud_key: Some("dedicated-key-1".to_string()),
        api_key: Some("generic-key-2".to_string()),
        ..Default::default()
    };
    let resolved = resolver
        .resolve_api_key_with_env(
            &args.to_provider_options(true),
            || Some("env-translate-3".to_string()),
            || Some("env-google-4".to_string()),
        )
        .unwrap();

    assert_eq!(resolved, "dedicated-key-1");
}

#[test]
fn test_google_cloud_precedence_cloud_key_whitespace_falls_back_to_api_key() {
    let resolver = GoogleCloudOptionResolver;
    let args = CliArgs {
        cloud_key: Some("   ".to_string()),
        api_key: Some("generic-key-2".to_string()),
        ..Default::default()
    };
    let resolved = resolver
        .resolve_api_key_with_env(
            &args.to_provider_options(true),
            || Some("env-translate-3".to_string()),
            || Some("env-google-4".to_string()),
        )
        .unwrap();

    assert_eq!(resolved, "generic-key-2");
}

#[test]
fn test_google_cloud_precedence_api_key_beats_env_vars() {
    let resolver = GoogleCloudOptionResolver;
    let args = CliArgs {
        cloud_key: None,
        api_key: Some("generic-key-2".to_string()),
        ..Default::default()
    };
    let resolved = resolver
        .resolve_api_key_with_env(
            &args.to_provider_options(true),
            || Some("env-translate-3".to_string()),
            || Some("env-google-4".to_string()),
        )
        .unwrap();

    assert_eq!(resolved, "generic-key-2");
}

#[test]
fn test_google_cloud_precedence_api_key_whitespace_falls_back_to_translate_env() {
    let resolver = GoogleCloudOptionResolver;
    let args = CliArgs {
        cloud_key: None,
        api_key: Some("   ".to_string()),
        ..Default::default()
    };
    let resolved = resolver
        .resolve_api_key_with_env(
            &args.to_provider_options(true),
            || Some("env-translate-3".to_string()),
            || Some("env-google-4".to_string()),
        )
        .unwrap();

    assert_eq!(resolved, "env-translate-3");
}

#[test]
fn test_google_cloud_precedence_translate_env_beats_google_env() {
    let resolver = GoogleCloudOptionResolver;
    let args = CliArgs {
        cloud_key: None,
        api_key: None,
        ..Default::default()
    };
    let resolved = resolver
        .resolve_api_key_with_env(
            &args.to_provider_options(true),
            || Some("env-translate-3".to_string()),
            || Some("env-google-4".to_string()),
        )
        .unwrap();

    assert_eq!(resolved, "env-translate-3");
}

#[test]
fn test_google_cloud_precedence_translate_env_whitespace_falls_back_to_google_env() {
    let resolver = GoogleCloudOptionResolver;
    let args = CliArgs {
        cloud_key: None,
        api_key: None,
        ..Default::default()
    };
    let resolved = resolver
        .resolve_api_key_with_env(
            &args.to_provider_options(true),
            || Some("   ".to_string()),
            || Some("env-google-4".to_string()),
        )
        .unwrap();

    assert_eq!(resolved, "env-google-4");
}

#[test]
fn test_google_cloud_precedence_all_empty_headless_returns_empty_string() {
    let resolver = GoogleCloudOptionResolver;
    let args = CliArgs {
        cloud_key: None,
        api_key: None,
        ..Default::default()
    };
    let resolved = resolver
        .resolve_api_key_with_env(&args.to_provider_options(true), || None, || None)
        .unwrap();

    assert_eq!(resolved, "");
}

#[test]
fn test_antigravity_resolver_when_empty_or_whitespace_model_returns_none() {
    let resolver = AntigravityOptionResolver;
    let args = create_test_args(Some("   ".to_string()), None);
    let config = resolver.resolve(&args.to_provider_options(false)).unwrap();

    assert_eq!(config.model(), None);
    assert_eq!(config.provider_kind(), TranslationProviderKind::Antigravity);
}

#[test]
fn test_antigravity_resolver_when_default_keyword_returns_none() {
    let resolver = AntigravityOptionResolver;
    let args = create_test_args(Some("default".to_string()), None);
    let config = resolver.resolve(&args.to_provider_options(false)).unwrap();

    assert_eq!(config.model(), None);
    assert_eq!(config.provider_kind(), TranslationProviderKind::Antigravity);
}

#[test]
fn test_antigravity_resolver_when_headless_and_model_is_none_returns_none() {
    let resolver = AntigravityOptionResolver;
    let args = create_test_args(None, None);
    let config = resolver.resolve(&args.to_provider_options(true)).unwrap();

    assert_eq!(config.model(), None);
    assert_eq!(config.provider_kind(), TranslationProviderKind::Antigravity);
}

#[test]
fn test_google_free_resolver_always_returns_google_free_variant() {
    let resolver = GoogleFreeOptionResolver;
    let args = create_test_args(None, None);

    let headless_config = resolver.resolve(&args.to_provider_options(true)).unwrap();
    assert_eq!(headless_config, ProviderConfig::GoogleFree);
    assert_eq!(
        headless_config.provider_kind(),
        TranslationProviderKind::GoogleFree
    );
    assert_eq!(headless_config.model(), None);
    assert_eq!(headless_config.api_key(), None);

    let interactive_config = resolver.resolve(&args.to_provider_options(false)).unwrap();
    assert_eq!(interactive_config, ProviderConfig::GoogleFree);
}

#[test]
fn test_google_cloud_resolver_when_api_key_provided_returns_key() {
    let resolver = GoogleCloudOptionResolver;
    let args = create_test_args(None, Some("secret_token_123".to_string()));

    let headless_config = resolver.resolve(&args.to_provider_options(true)).unwrap();
    assert_eq!(headless_config.api_key(), Some("secret_token_123"));
    assert_eq!(
        headless_config.provider_kind(),
        TranslationProviderKind::GoogleCloud
    );

    let interactive_config = resolver.resolve(&args.to_provider_options(false)).unwrap();
    assert_eq!(interactive_config.api_key(), Some("secret_token_123"));
}

#[test]
fn test_antigravity_resolver_when_headless_and_model_provided_returns_model() {
    let resolver = AntigravityOptionResolver;
    let args = create_test_args(Some("gemini-4-argon-high".to_string()), None);
    let config = resolver.resolve(&args.to_provider_options(true)).unwrap();

    assert_eq!(config.model(), Some("gemini-4-argon-high"));
    assert_eq!(config.provider_kind(), TranslationProviderKind::Antigravity);
}

#[test]
fn test_antigravity_resolver_when_interactive_and_model_provided_returns_model() {
    let resolver = AntigravityOptionResolver;
    let args = create_test_args(Some("gemini-3.8-flash-high".to_string()), None);
    let config = resolver.resolve(&args.to_provider_options(false)).unwrap();

    assert_eq!(config.model(), Some("gemini-3.8-flash-high"));
    assert_eq!(config.provider_kind(), TranslationProviderKind::Antigravity);
}

#[test]
fn test_provider_config_resolver_dispatch_all_providers() {
    let args = create_test_args(
        Some("gemini-4-argon-medium".to_string()),
        Some("gcp-key-456".to_string()),
    );

    let agy_config =
        ProviderConfigResolver::resolve(TranslationProviderKind::Antigravity, &args, true).unwrap();
    assert_eq!(
        agy_config,
        ProviderConfig::Antigravity {
            model: Some("gemini-4-argon-medium".to_string()),
            block_concurrency: Some(2),
        }
    );

    let free_config =
        ProviderConfigResolver::resolve(TranslationProviderKind::GoogleFree, &args, true).unwrap();
    assert_eq!(free_config, ProviderConfig::GoogleFree);

    let cloud_config =
        ProviderConfigResolver::resolve(TranslationProviderKind::GoogleCloud, &args, true).unwrap();
    assert_eq!(
        cloud_config,
        ProviderConfig::GoogleCloud {
            api_key: "gcp-key-456".to_string()
        }
    );
}

#[test]
fn test_provider_config_constructors_and_helpers() {
    let agy = ProviderConfig::antigravity(Some("custom-model".to_string()));
    assert_eq!(agy.provider_kind(), TranslationProviderKind::Antigravity);
    assert_eq!(agy.model(), Some("custom-model"));
    assert_eq!(agy.api_key(), None);

    let free = ProviderConfig::google_free();
    assert_eq!(free.provider_kind(), TranslationProviderKind::GoogleFree);
    assert_eq!(free.model(), None);
    assert_eq!(free.api_key(), None);

    let cloud = ProviderConfig::google_cloud("cloud-api-key");
    assert_eq!(cloud.provider_kind(), TranslationProviderKind::GoogleCloud);
    assert_eq!(cloud.model(), None);
    assert_eq!(cloud.api_key(), Some("cloud-api-key"));

    let created_from_new = ProviderConfig::new(
        TranslationProviderKind::GoogleCloud,
        None,
        Some("from-new-key".to_string()),
    );
    assert_eq!(created_from_new.api_key(), Some("from-new-key"));
}
