use crate::adapters::cache::SqliteCacheAdapter;
use crate::adapters::cli::args::{CliArgs, HistoryFormat};
use crate::adapters::cli::interactive::InteractivePrompter;
use crate::adapters::cli::provider_resolver::ProviderConfigResolver;
use crate::adapters::epub::ZipEpubAdapter;
use crate::adapters::error_logger::{ErrorLogger, ExecutionContext};
use crate::adapters::progress::IndicatifProgressReporter;
use crate::adapters::translator::TranslatorFactory;
use crate::domain::entities::{ExecutionMetricsRecord, Language, TranslationProviderKind};
use crate::domain::errors::{AppError, EpubError};
use crate::domain::ports::{CachePort, EpubPort, ProgressReporterPort};
use crate::domain::services::TranslateBookUseCase;
use std::path::Path;
use std::sync::Arc;

pub struct CliRunner;

impl CliRunner {
    pub async fn run(args: CliArgs) -> Result<(), AppError> {
        if let Some(verify_path) = args.is_verify_requested() {
            return Self::run_verify(verify_path).await;
        }

        if let Some((limit, format)) = args.is_history_requested() {
            return Self::render_history(&args.db_cache, limit, format).await;
        }

        let mut ctx = ExecutionContext::default();
        if let Err(err) = Self::execute_pipeline(args, &mut ctx).await {
            Self::handle_error(&err, &ctx);
            return Err(err);
        }
        Ok(())
    }

    pub async fn run_verify(path: &Path) -> Result<(), AppError> {
        let epub_adapter = ZipEpubAdapter::new();
        let report = epub_adapter.verify_epub(path).await?;
        if report.is_valid {
            println!("EPUB verification successful: {}", path.display());
            println!("  - OCF Mimetype: Valid (Stored, 20 bytes)");
            println!("  - container.xml: Valid");
            println!("  - OPF Package: Valid");
            println!(
                "  - XHTML files: {}/{} valid (100% compliant)",
                report.valid_xhtml_files, report.total_xhtml_files
            );
            Ok(())
        } else {
            eprintln!("EPUB verification failed: {}", path.display());
            for err in &report.errors {
                eprintln!("  - {err}");
            }
            eprintln!(
                "Summary: {}/{} XHTML files valid",
                report.valid_xhtml_files, report.total_xhtml_files
            );
            Err(AppError::Epub(EpubError::VerificationFailed(format!(
                "EPUB failed integrity check with {} error(s)",
                report.errors.len()
            ))))
        }
    }

    pub async fn render_history(
        db_path: &Path,
        limit: usize,
        format: HistoryFormat,
    ) -> Result<(), AppError> {
        let cache = SqliteCacheAdapter::new(db_path)?;
        let records = cache.get_metrics_history(limit).await?;
        match format {
            HistoryFormat::Table => {
                let table = Self::format_history_table(&records);
                println!("{table}");
            }
            HistoryFormat::Json => {
                let json_str = serde_json::to_string_pretty(&records)
                    .map_err(|e| AppError::Config(format!("Failed to format JSON metrics: {e}")))?;
                println!("{json_str}");
            }
            HistoryFormat::Csv => {
                let csv_str = Self::format_history_csv(&records);
                println!("{csv_str}");
            }
        }
        Ok(())
    }

    pub fn format_history_csv(records: &[ExecutionMetricsRecord]) -> String {
        let mut out = String::from(
            "id,timestamp,provider,target_lang,total_chapters,cache_hits,cache_misses,hit_ratio_pct,avg_latency_ms,total_duration_secs\n",
        );
        for r in records {
            out.push_str(&format!(
                "{},\"{}\",\"{}\",\"{}\",{},{},{},{:.1},{:.1},{:.2}\n",
                r.id,
                r.timestamp,
                r.provider,
                r.target_lang,
                r.total_chapters,
                r.cache_hits,
                r.cache_misses,
                r.hit_ratio_pct,
                r.avg_latency_ms,
                r.total_duration_secs
            ));
        }
        out.trim_end().to_string()
    }

    pub fn format_history_table(records: &[ExecutionMetricsRecord]) -> String {
        if records.is_empty() {
            return "Nenhum registro de métricas encontrado no histórico.".to_string();
        }

        let headers = [
            "ID",
            "Data/Hora",
            "Provedor",
            "Destino",
            "Capítulos",
            "Hits",
            "Misses",
            "Hit Ratio",
            "Latência Média",
            "Duração Total",
        ];

        let mut rows = Vec::with_capacity(records.len());
        for r in records {
            rows.push([
                r.id.to_string(),
                r.timestamp.clone(),
                r.provider.clone(),
                r.target_lang.clone(),
                r.total_chapters.to_string(),
                r.cache_hits.to_string(),
                r.cache_misses.to_string(),
                format!("{:.1}%", r.hit_ratio_pct),
                format!("{:.1}ms", r.avg_latency_ms),
                format!("{:.2}s", r.total_duration_secs),
            ]);
        }

        let mut widths = [0usize; 10];
        for (i, h) in headers.iter().enumerate() {
            widths[i] = h.len();
        }
        for row in &rows {
            for (i, cell) in row.iter().enumerate() {
                widths[i] = widths[i].max(cell.len());
            }
        }

        let mut out = String::new();
        for (i, h) in headers.iter().enumerate() {
            if i > 0 {
                out.push_str(" | ");
            }
            out.push_str(&format!("{:width$}", h, width = widths[i]));
        }
        out.push('\n');

        for (i, w) in widths.iter().enumerate() {
            if i > 0 {
                out.push_str("-+-");
            }
            out.push_str(&"-".repeat(*w));
        }
        out.push('\n');

        for row in &rows {
            for (i, cell) in row.iter().enumerate() {
                if i > 0 {
                    out.push_str(" | ");
                }
                out.push_str(&format!("{:width$}", cell, width = widths[i]));
            }
            out.push('\n');
        }

        out.trim_end().to_string()
    }

    fn handle_error(err: &AppError, ctx: &ExecutionContext) {
        match err {
            AppError::UserCancelled => {
                eprintln!("\nOperação cancelada pelo usuário.");
            }
            other => {
                eprintln!("\nErro fatal durante a execução: {other}");
                match ErrorLogger::log_error(other, ctx) {
                    Ok(log_path) => {
                        eprintln!(
                            "Relatório detalhado do erro gravado em: {}",
                            log_path.display()
                        );
                    }
                    Err(log_err) => {
                        eprintln!("Falha ao gravar arquivo de log de erro: {log_err}");
                    }
                }
            }
        }
    }

    async fn execute_pipeline(args: CliArgs, ctx: &mut ExecutionContext) -> Result<(), AppError> {
        ctx.cache_db = Some(args.db_cache.clone());

        let resolved_input = args
            .input
            .as_ref()
            .or(args.positional_input.as_ref())
            .cloned();

        let is_headless = args.headless
            || (resolved_input.is_some()
                && args.output.is_some()
                && args.target.is_some()
                && args.provider.is_some());

        let input_path = if is_headless {
            let p = resolved_input.ok_or_else(|| {
                AppError::Config(
                    "Parâmetro obrigatório ausente em modo headless: --input (-i)".to_string(),
                )
            })?;
            if !p.exists() {
                return Err(AppError::Config(format!(
                    "Arquivo de entrada não encontrado: {}",
                    p.display()
                )));
            }
            p
        } else {
            InteractivePrompter::prompt_input_path(resolved_input)?
        };
        ctx.input_path = Some(input_path.clone());

        let source_lang = if is_headless {
            Language::new(args.source.as_deref().unwrap_or("auto"))
        } else {
            InteractivePrompter::prompt_source_language(args.source.clone())?
        };
        ctx.source_lang = Some(source_lang.as_str().to_string());

        let target_lang = if is_headless {
            Language::new(args.target.as_ref().ok_or_else(|| {
                AppError::Config(
                    "Parâmetro obrigatório ausente em modo headless: --target (-t)".to_string(),
                )
            })?)
        } else {
            InteractivePrompter::prompt_target_language(args.target.clone())?
        };
        ctx.target_lang = Some(target_lang.as_str().to_string());

        let provider_kind = if is_headless {
            let prov_str = args.provider.as_deref().unwrap_or("antigravity");
            prov_str
                .parse::<TranslationProviderKind>()
                .map_err(AppError::Config)?
        } else {
            InteractivePrompter::prompt_provider(args.provider.clone())?
        };
        ctx.provider = Some(provider_kind.as_str().to_string());

        let provider_config =
            ProviderConfigResolver::resolve_interactive(provider_kind, &args, is_headless).await?;
        ctx.model = provider_config.model().map(ToString::to_string);

        let output_path = if is_headless {
            args.output.clone().unwrap_or_else(|| {
                InteractivePrompter::compute_default_output(&input_path, &target_lang)
            })
        } else {
            InteractivePrompter::prompt_output_path(args.output.clone(), &input_path, &target_lang)?
        };
        ctx.output_path = Some(output_path.clone());

        let epub_adapter: Arc<dyn EpubPort> = Arc::new(ZipEpubAdapter::new());
        let cache_adapter: Arc<dyn CachePort> = Arc::new(SqliteCacheAdapter::new(&args.db_cache)?);
        let translator_adapter =
            TranslatorFactory::create_with_cache(&provider_config, Some(cache_adapter.clone()))?;
        let progress_reporter: Arc<dyn ProgressReporterPort> =
            Arc::new(IndicatifProgressReporter::new());

        let use_case = TranslateBookUseCase::with_concurrency(
            epub_adapter.clone(),
            translator_adapter,
            cache_adapter,
            Some(progress_reporter),
            args.concurrency as usize,
        );

        let _metrics = use_case
            .execute(&input_path, &output_path, &source_lang, &target_lang)
            .await?;

        println!(
            "\nEPUB traduzido salvo com sucesso em: {}",
            output_path.display()
        );

        println!("Executando validação de integridade pós-tradução...");
        let report = epub_adapter.verify_epub(&output_path).await?;
        if !report.is_valid {
            for err in &report.errors {
                eprintln!("  - {err}");
            }
            return Err(AppError::Epub(EpubError::VerificationFailed(format!(
                "Translated EPUB failed post-translation integrity check with {} error(s)",
                report.errors.len()
            ))));
        }
        println!(
            "Validação de integridade concluída com sucesso: {} arquivos XHTML 100% íntegros.",
            report.total_xhtml_files
        );

        Ok(())
    }
}
