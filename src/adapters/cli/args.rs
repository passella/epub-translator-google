use crate::adapters::translator::ProviderOptions;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum HistoryFormat {
    #[default]
    Table,
    Json,
    Csv,
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum CliCommand {
    History {
        #[arg(short = 'l', long, default_value = "10", value_name = "LIMITE")]
        limit: usize,

        #[arg(short = 'f', long, default_value = "table", value_name = "FORMATO")]
        format: HistoryFormat,
    },
    Verify {
        #[arg(value_name = "ARQUIVO_EPUB")]
        path: PathBuf,
    },
}

#[derive(Parser, Debug, Clone)]
#[command(name = "epub-translator-google")]
#[command(version = "0.1.0")]
#[command(
    about = "Tradutor profissional de e-books EPUB com Google Translate e Google Antigravity"
)]
pub struct CliArgs {
    #[command(subcommand)]
    pub command: Option<CliCommand>,

    #[arg(short, long, value_name = "ARQUIVO_EPUB")]
    pub input: Option<PathBuf>,

    #[arg(value_name = "ARQUIVO_EPUB_POS")]
    pub positional_input: Option<PathBuf>,

    #[arg(short, long, value_name = "SAIDA")]
    pub output: Option<PathBuf>,

    #[arg(
        short,
        long,
        visible_alias = "source-lang",
        value_name = "IDIOMA_ORIGEM"
    )]
    pub source: Option<String>,

    #[arg(
        short,
        long,
        visible_alias = "target-lang",
        value_name = "IDIOMA_DESTINO"
    )]
    pub target: Option<String>,

    #[arg(short, long, value_name = "PROVEDOR")]
    pub provider: Option<String>,

    #[arg(long, value_name = "MODELO")]
    pub model: Option<String>,

    #[arg(
        long,
        default_value = "translations_cache.db",
        value_name = "ARQUIVO_DB"
    )]
    pub db_cache: PathBuf,

    #[arg(long, value_name = "API_KEY")]
    pub api_key: Option<String>,

    #[arg(long, visible_alias = "cloud-api-key", value_name = "CLOUD_KEY")]
    pub cloud_key: Option<String>,

    #[arg(
        short = 'c',
        long,
        default_value = "4",
        value_parser = clap::value_parser!(u32).range(1..),
        value_name = "CONCORRENCIA"
    )]
    pub concurrency: u32,

    #[arg(long)]
    pub headless: bool,

    #[arg(
        long,
        default_value = "2",
        value_parser = clap::value_parser!(u32).range(1..),
        value_name = "BLOCK_CONCORRENCIA"
    )]
    pub block_concurrency: u32,

    #[arg(long)]
    pub history: bool,

    #[arg(short = 'l', long, default_value = "10", value_name = "LIMITE")]
    pub limit: usize,

    #[arg(short = 'f', long, value_name = "FORMATO")]
    pub format: Option<HistoryFormat>,
}

impl CliArgs {
    pub fn is_history_requested(&self) -> Option<(usize, HistoryFormat)> {
        if let Some(CliCommand::History { limit, format }) = self.command {
            Some((limit, format))
        } else if self.history {
            Some((self.limit, self.format.unwrap_or_default()))
        } else {
            None
        }
    }

    pub fn is_verify_requested(&self) -> Option<&Path> {
        if let Some(CliCommand::Verify { ref path }) = self.command {
            Some(path.as_path())
        } else {
            None
        }
    }

    pub fn to_provider_options(&self, is_headless: bool) -> ProviderOptions {
        ProviderOptions {
            model: self.model.clone(),
            block_concurrency: self.block_concurrency as usize,
            api_key: self.api_key.clone(),
            cloud_key: self.cloud_key.clone(),
            is_headless,
        }
    }
}

impl Default for CliArgs {
    fn default() -> Self {
        Self {
            command: None,
            input: None,
            positional_input: None,
            output: None,
            source: None,
            target: None,
            provider: None,
            model: None,
            db_cache: PathBuf::from("translations_cache.db"),
            api_key: None,
            cloud_key: None,
            concurrency: 4,
            headless: false,
            block_concurrency: 2,
            history: false,
            limit: 10,
            format: None,
        }
    }
}
