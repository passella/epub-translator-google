mod common;

use clap::Parser;
use common::TempDirGuard;
use epub_translator_google::adapters::cache::SqliteCacheAdapter;
use epub_translator_google::adapters::cli::{
    CliArgs, CliCommand, CliRunner, HistoryFormat, ProviderConfigResolver,
};
use epub_translator_google::adapters::translator::{
    AntigravityTranslatorAdapter, TranslatorFactory,
};
use epub_translator_google::domain::entities::{
    ChapterLatency, ExecutionMetrics, ExecutionMetricsRecord, TranslationProviderKind,
};
use epub_translator_google::domain::errors::AppError;
use epub_translator_google::domain::ports::CachePort;
use std::path::PathBuf;
use std::time::Duration;

#[test]
fn test_cli_args_concurrency_zero_rejected_syntactically() {
    let result = CliArgs::try_parse_from(["app", "-c", "0"]);
    assert!(result.is_err());

    let long_result = CliArgs::try_parse_from(["app", "--concurrency", "0"]);
    assert!(long_result.is_err());
}

#[test]
fn test_cli_args_block_concurrency_zero_rejected_syntactically() {
    let result = CliArgs::try_parse_from(["app", "--block-concurrency", "0"]);
    assert!(result.is_err());
}

#[test]
fn test_cli_args_concurrency_and_block_concurrency_boundary_values() {
    let min_args = CliArgs::try_parse_from(["app", "-c", "1", "--block-concurrency", "1"]).unwrap();
    assert_eq!(min_args.concurrency, 1);
    assert_eq!(min_args.block_concurrency, 1);

    let default_args = CliArgs::try_parse_from(["app"]).unwrap();
    assert_eq!(default_args.concurrency, 4);
    assert_eq!(default_args.block_concurrency, 2);
    assert!(!default_args.headless);
    assert!(!default_args.history);
    assert_eq!(default_args.limit, 10);
    assert_eq!(default_args.is_history_requested(), None);

    let custom_args =
        CliArgs::try_parse_from(["app", "-c", "16", "--block-concurrency", "8"]).unwrap();
    assert_eq!(custom_args.concurrency, 16);
    assert_eq!(custom_args.block_concurrency, 8);
}

#[test]
fn test_cli_args_headless_flag_parsing() {
    let args = CliArgs::try_parse_from(["app", "--headless"]).unwrap();
    assert!(args.headless);

    let default_args = CliArgs::try_parse_from(["app"]).unwrap();
    assert!(!default_args.headless);
}

#[test]
fn test_cli_args_history_subcommand_and_flag_parsing() {
    let sub_default = CliArgs::try_parse_from(["app", "history"]).unwrap();
    assert_eq!(
        sub_default.is_history_requested(),
        Some((10, HistoryFormat::Table))
    );
    assert_eq!(
        sub_default.command,
        Some(CliCommand::History {
            limit: 10,
            format: HistoryFormat::Table,
        })
    );

    let sub_custom =
        CliArgs::try_parse_from(["app", "history", "--limit", "25", "--format", "json"]).unwrap();
    assert_eq!(
        sub_custom.is_history_requested(),
        Some((25, HistoryFormat::Json))
    );
    assert_eq!(
        sub_custom.command,
        Some(CliCommand::History {
            limit: 25,
            format: HistoryFormat::Json,
        })
    );

    let sub_csv =
        CliArgs::try_parse_from(["app", "history", "-l", "5", "--format", "csv"]).unwrap();
    assert_eq!(
        sub_csv.is_history_requested(),
        Some((5, HistoryFormat::Csv))
    );

    let flag_default = CliArgs::try_parse_from(["app", "--history"]).unwrap();
    assert!(flag_default.history);
    assert_eq!(
        flag_default.is_history_requested(),
        Some((10, HistoryFormat::Table))
    );

    let flag_custom =
        CliArgs::try_parse_from(["app", "--history", "--limit", "30", "--format", "json"]).unwrap();
    assert!(flag_custom.history);
    assert_eq!(
        flag_custom.is_history_requested(),
        Some((30, HistoryFormat::Json))
    );

    let flag_csv =
        CliArgs::try_parse_from(["app", "--history", "-l", "7", "--format", "csv"]).unwrap();
    assert_eq!(
        flag_csv.is_history_requested(),
        Some((7, HistoryFormat::Csv))
    );
}

#[test]
fn test_cli_args_verify_subcommand_parsing() {
    let args = CliArgs::try_parse_from(["app", "verify", "book.epub"]).unwrap();
    assert_eq!(
        args.command,
        Some(CliCommand::Verify {
            path: PathBuf::from("book.epub")
        })
    );
    assert_eq!(
        args.is_verify_requested(),
        Some(PathBuf::from("book.epub").as_path())
    );

    let no_verify = CliArgs::try_parse_from(["app"]).unwrap();
    assert_eq!(no_verify.is_verify_requested(), None);
}

#[test]
fn test_provider_resolver_propagates_block_concurrency_to_antigravity_config() {
    let args = CliArgs {
        block_concurrency: 5,
        ..Default::default()
    };

    let config =
        ProviderConfigResolver::resolve(TranslationProviderKind::Antigravity, &args, true).unwrap();
    assert_eq!(config.block_concurrency(), Some(5));

    let adapter =
        TranslatorFactory::create_with_cache(&config, None).expect("Failed to create translator");
    assert_eq!(adapter.provider_name(), "antigravity");

    let direct_adapter = AntigravityTranslatorAdapter::new(None)
        .with_block_concurrency(config.block_concurrency().unwrap());
    assert_eq!(direct_adapter.block_concurrency(), 5);
}

#[test]
fn test_format_history_table_empty_records() {
    let output = CliRunner::format_history_table(&[]);
    assert_eq!(
        output,
        "Nenhum registro de métricas encontrado no histórico."
    );
}

#[test]
fn test_format_history_csv_empty_records() {
    let output = CliRunner::format_history_csv(&[]);
    assert_eq!(
        output,
        "id,timestamp,provider,target_lang,total_chapters,cache_hits,cache_misses,hit_ratio_pct,avg_latency_ms,total_duration_secs"
    );
}

#[test]
fn test_format_history_csv_single_and_multiple_records() {
    let records = vec![ExecutionMetricsRecord {
        id: 1,
        timestamp: "2026-11-11 12:00:00".to_string(),
        provider: "antigravity".to_string(),
        target_lang: "pt-BR".to_string(),
        total_chapters: 10,
        cache_hits: 8,
        cache_misses: 2,
        hit_ratio_pct: 80.0,
        avg_latency_ms: 150.5,
        total_duration_secs: 5.25,
    }];

    let csv = CliRunner::format_history_csv(&records);
    assert!(csv.contains("id,timestamp,provider,target_lang"));
    assert!(
        csv.contains("1,\"2026-11-11 12:00:00\",\"antigravity\",\"pt-BR\",10,8,2,80.0,150.5,5.25")
    );
}

#[test]
fn test_format_history_table_single_and_multiple_records() {
    let records = vec![
        ExecutionMetricsRecord {
            id: 1,
            timestamp: "2026-11-11 12:00:00".to_string(),
            provider: "antigravity".to_string(),
            target_lang: "pt-BR".to_string(),
            total_chapters: 10,
            cache_hits: 8,
            cache_misses: 2,
            hit_ratio_pct: 80.0,
            avg_latency_ms: 150.5,
            total_duration_secs: 5.25,
        },
        ExecutionMetricsRecord {
            id: 2,
            timestamp: "2026-11-11 13:00:00".to_string(),
            provider: "google-free".to_string(),
            target_lang: "es".to_string(),
            total_chapters: 5,
            cache_hits: 5,
            cache_misses: 0,
            hit_ratio_pct: 100.0,
            avg_latency_ms: 0.0,
            total_duration_secs: 1.10,
        },
    ];

    let output = CliRunner::format_history_table(&records);
    assert!(output.contains("ID"));
    assert!(output.contains("Data/Hora"));
    assert!(output.contains("Provedor"));
    assert!(output.contains("Destino"));
    assert!(output.contains("Capítulos"));
    assert!(output.contains("Hits"));
    assert!(output.contains("Misses"));
    assert!(output.contains("Hit Ratio"));
    assert!(output.contains("Latência Média"));
    assert!(output.contains("Duração Total"));

    assert!(output.contains("antigravity"));
    assert!(output.contains("pt-BR"));
    assert!(output.contains("80.0%"));
    assert!(output.contains("150.5ms"));
    assert!(output.contains("5.25s"));

    assert!(output.contains("google-free"));
    assert!(output.contains("es"));
    assert!(output.contains("100.0%"));
    assert!(output.contains("0.0ms"));
    assert!(output.contains("1.10s"));
}

#[tokio::test]
async fn test_render_history_empty_database_all_formats() {
    let temp_dir = TempDirGuard::new("cli_runner_history_empty");
    let temp_db = temp_dir.path().join("history_empty_formats.db");

    assert!(
        CliRunner::render_history(&temp_db, 10, HistoryFormat::Table)
            .await
            .is_ok()
    );
    assert!(
        CliRunner::render_history(&temp_db, 10, HistoryFormat::Json)
            .await
            .is_ok()
    );
    assert!(
        CliRunner::render_history(&temp_db, 10, HistoryFormat::Csv)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn test_render_history_with_recorded_metrics() {
    let temp_dir = TempDirGuard::new("cli_runner_history_records");
    let temp_db = temp_dir.path().join("history_records.db");

    let cache = SqliteCacheAdapter::new(&temp_db).unwrap();
    let metrics = ExecutionMetrics {
        total_chapters: 4,
        cache_hits: 3,
        cache_misses: 1,
        empty_chapters: 0,
        chapter_latencies: vec![ChapterLatency {
            index: 0,
            path: "chap1.xhtml".to_string(),
            latency: Duration::from_millis(200),
        }],
        total_elapsed: Duration::from_millis(850),
    };

    cache
        .record_metrics("antigravity", "pt-BR", &metrics)
        .await
        .unwrap();

    let result = CliRunner::render_history(&temp_db, 5, HistoryFormat::Table).await;
    assert!(result.is_ok());

    let run_flag_result = CliRunner::run(CliArgs {
        db_cache: temp_db.clone(),
        history: true,
        limit: 5,
        format: Some(HistoryFormat::Json),
        ..Default::default()
    })
    .await;
    assert!(run_flag_result.is_ok());

    let run_sub_result = CliRunner::run(CliArgs {
        db_cache: temp_db.clone(),
        command: Some(CliCommand::History {
            limit: 5,
            format: HistoryFormat::Csv,
        }),
        ..Default::default()
    })
    .await;
    assert!(run_sub_result.is_ok());
}

#[tokio::test]
async fn test_cli_runner_headless_missing_input_returns_error() {
    let args = CliArgs {
        headless: true,
        input: None,
        target: Some("pt-BR".to_string()),
        ..Default::default()
    };

    let result = CliRunner::run(args).await;
    match result {
        Err(AppError::Config(msg)) => {
            assert!(msg.contains("Parâmetro obrigatório ausente em modo headless: --input"));
        }
        other => panic!("Expected AppError::Config, got {other:?}"),
    }
}

#[tokio::test]
async fn test_cli_runner_headless_missing_target_returns_error() {
    let temp_dir = TempDirGuard::new("cli_runner_missing_target");
    let temp_input = temp_dir.path().join("missing_target.epub");
    std::fs::write(&temp_input, b"dummy epub content").unwrap();

    let args = CliArgs {
        headless: true,
        input: Some(temp_input),
        target: None,
        ..Default::default()
    };

    let result = CliRunner::run(args).await;
    match result {
        Err(AppError::Config(msg)) => {
            assert!(msg.contains("Parâmetro obrigatório ausente em modo headless: --target"));
        }
        other => panic!("Expected AppError::Config, got {other:?}"),
    }
}

#[tokio::test]
async fn test_cli_runner_headless_missing_file_returns_error() {
    let temp_dir = TempDirGuard::new("cli_runner_missing_file");
    let missing_input = temp_dir.path().join("non_existent_file_xyz123.epub");
    let temp_output = temp_dir.path().join("output.epub");

    let args = CliArgs {
        headless: true,
        input: Some(missing_input),
        output: Some(temp_output),
        source: Some("en".to_string()),
        target: Some("pt-BR".to_string()),
        provider: Some("google-free".to_string()),
        ..Default::default()
    };

    let result = CliRunner::run(args).await;
    match result {
        Err(AppError::Config(msg)) => {
            assert!(msg.contains("Arquivo de entrada não encontrado"));
        }
        other => panic!("Expected AppError::Config, got {other:?}"),
    }
}

#[tokio::test]
async fn test_cli_runner_headless_invalid_provider_returns_error() {
    let temp_dir = TempDirGuard::new("cli_runner_invalid_provider");
    let temp_input = temp_dir.path().join("input.epub");
    let temp_output = temp_dir.path().join("output.epub");
    std::fs::write(&temp_input, b"dummy epub content").unwrap();

    let args = CliArgs {
        headless: true,
        input: Some(temp_input),
        output: Some(temp_output),
        source: Some("en".to_string()),
        target: Some("pt-BR".to_string()),
        provider: Some("invalid-provider-xyz".to_string()),
        ..Default::default()
    };

    let result = CliRunner::run(args).await;
    assert!(result.is_err());
}

#[test]
fn test_cli_args_db_cache_global_before_and_after_subcommands() {
    let missing_value = CliArgs::try_parse_from(["app", "history", "--db-cache"]);
    assert!(missing_value.is_err());

    let default_history = CliArgs::try_parse_from(["app", "history"]).unwrap();
    assert_eq!(
        default_history.db_cache,
        PathBuf::from("translations_cache.db")
    );

    let before_history = CliArgs::try_parse_from([
        "app",
        "--db-cache",
        "/tmp/before_history.db",
        "history",
        "-l",
        "5",
        "-f",
        "json",
    ])
    .unwrap();
    assert_eq!(
        before_history.db_cache,
        PathBuf::from("/tmp/before_history.db")
    );
    assert_eq!(
        before_history.is_history_requested(),
        Some((5, HistoryFormat::Json))
    );

    let after_history =
        CliArgs::try_parse_from(["app", "history", "--db-cache", "/tmp/after_history.db"]).unwrap();
    assert_eq!(
        after_history.db_cache,
        PathBuf::from("/tmp/after_history.db")
    );
    assert_eq!(
        after_history.is_history_requested(),
        Some((10, HistoryFormat::Table))
    );

    let mid_history = CliArgs::try_parse_from([
        "app",
        "history",
        "-l",
        "3",
        "--db-cache",
        "/tmp/mid_history.db",
        "-f",
        "csv",
    ])
    .unwrap();
    assert_eq!(mid_history.db_cache, PathBuf::from("/tmp/mid_history.db"));
    assert_eq!(
        mid_history.is_history_requested(),
        Some((3, HistoryFormat::Csv))
    );

    let before_verify = CliArgs::try_parse_from([
        "app",
        "--db-cache",
        "/tmp/before_verify.db",
        "verify",
        "b.epub",
    ])
    .unwrap();
    assert_eq!(
        before_verify.db_cache,
        PathBuf::from("/tmp/before_verify.db")
    );
    assert_eq!(
        before_verify.is_verify_requested(),
        Some(PathBuf::from("b.epub").as_path())
    );

    let after_verify = CliArgs::try_parse_from([
        "app",
        "verify",
        "b.epub",
        "--db-cache",
        "/tmp/after_verify.db",
    ])
    .unwrap();
    assert_eq!(after_verify.db_cache, PathBuf::from("/tmp/after_verify.db"));
    assert_eq!(
        after_verify.is_verify_requested(),
        Some(PathBuf::from("b.epub").as_path())
    );
}

#[test]
fn test_cli_args_version_flag_matches_cargo_pkg_version() {
    let err = CliArgs::try_parse_from(["epub-translator-google", "--version"]).unwrap_err();
    assert_eq!(err.kind(), clap::error::ErrorKind::DisplayVersion);
    let rendered = err.to_string();
    assert!(rendered.contains(env!("CARGO_PKG_VERSION")));

    if let Some(bin_path) = option_env!("CARGO_BIN_EXE_epub-translator-google") {
        let output = std::process::Command::new(bin_path)
            .arg("--version")
            .output()
            .unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains(env!("CARGO_PKG_VERSION")));
    }
}
