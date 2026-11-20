use crate::adapters::error_logger::ErrorLogger;
use crate::domain::entities::ExecutionMetrics;
use crate::domain::ports::ProgressReporterPort;
use indicatif::{ProgressBar, ProgressStyle};
use std::sync::Mutex;

pub struct IndicatifProgressReporter {
    bar: Mutex<Option<ProgressBar>>,
}

impl IndicatifProgressReporter {
    pub fn new() -> Self {
        Self {
            bar: Mutex::new(None),
        }
    }
}

impl Default for IndicatifProgressReporter {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressReporterPort for IndicatifProgressReporter {
    fn on_start(&self, total_chapters: usize, book_title: &str) {
        let pb = ProgressBar::new(total_chapters as u64);
        let style = ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} capítulos ({msg})")
            .unwrap_or_else(|_| ProgressStyle::default_bar())
            .progress_chars("#>-");

        pb.set_style(style);
        let safe_title = ErrorLogger::sanitize_terminal_text(book_title);
        pb.set_message(format!("Iniciando: {safe_title}"));
        pb.enable_steady_tick(std::time::Duration::from_millis(100));

        let mut lock = self.bar.lock().unwrap_or_else(|p| p.into_inner());
        *lock = Some(pb);
    }

    fn on_chapter_start(&self, _index: usize, chapter_path: &str) {
        let lock = self.bar.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(pb) = lock.as_ref() {
            let safe_path = ErrorLogger::sanitize_terminal_text(chapter_path);
            pb.set_message(format!("Traduzindo {safe_path}"));
        }
    }

    fn on_chapter_complete(&self, _index: usize, _chapter_path: &str) {
        let lock = self.bar.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(pb) = lock.as_ref() {
            pb.inc(1);
        }
    }

    fn on_finish(&self, message: &str) {
        let lock = self.bar.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(pb) = lock.as_ref() {
            pb.finish_with_message(ErrorLogger::sanitize_terminal_text(message));
        }
    }

    fn on_metrics(&self, metrics: &ExecutionMetrics) {
        println!();
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("  📊 MÉTRICAS DE EXECUÇÃO & OBSERVABILIDADE");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("  • Total de Capítulos       : {}", metrics.total_chapters);
        println!("  • Recuperados do Cache     : {}", metrics.cache_hits);
        println!("  • Traduzidos via Provedor  : {}", metrics.cache_misses);
        if metrics.empty_chapters > 0 {
            println!("  • Capítulos Vazios         : {}", metrics.empty_chapters);
        }
        println!(
            "  • Taxa de Acerto de Cache  : {:.2}%",
            metrics.cache_hit_ratio()
        );

        if metrics.cache_misses > 0 {
            let avg = metrics.avg_provider_latency();
            println!(
                "  • Latência Média Provedor  : {:.2}s ({} ms)",
                avg.as_secs_f64(),
                avg.as_millis()
            );
            if let (Some(min), Some(max)) = (
                metrics.min_provider_latency(),
                metrics.max_provider_latency(),
            ) {
                println!(
                    "  • Latência Min / Max       : {:.2}s / {:.2}s",
                    min.as_secs_f64(),
                    max.as_secs_f64()
                );
            }
        } else {
            println!("  • Latência Média Provedor  : N/A (100% recuperado do cache)");
        }

        println!(
            "  • Tempo Total Decorrido    : {:.2}s",
            metrics.total_elapsed.as_secs_f64()
        );

        if !metrics.chapter_latencies.is_empty() {
            println!("  ────────────────────────────────────────────────────────────");
            println!("  Latência Individual das Chamadas ao Provedor:");
            for entry in &metrics.chapter_latencies {
                println!(
                    "    - [{}] {}: {:.2}s ({} ms)",
                    entry.index + 1,
                    ErrorLogger::sanitize_terminal_text(&entry.path),
                    entry.latency.as_secs_f64(),
                    entry.latency.as_millis()
                );
            }
        }
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    }
}
