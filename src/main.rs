use clap::Parser;
use epub_translator_google::adapters::cli::{CliArgs, CliRunner};
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let args = CliArgs::parse();
    match CliRunner::run(args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
