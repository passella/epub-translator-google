# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.1] - 2026-11-26

### Security
- Patched `RUSTSEC-2026-0592` by updating `url` from `2.5.8` to `2.5.9` (and transitive `form_urlencoded` from `1.2.2` to `1.2.3`) in `Cargo.lock`.
- Added `cargo-deny` policy (`deny.toml`) across CI and release workflows alongside `zizmor` / `actionlint` workflow auditing and automated `cargo-audit` / Dependabot dependency monitoring.

### Changed
- Bumped package version from `1.0.0` to `1.0.1` in `Cargo.toml` and `Cargo.lock`.
- Added shared RAII `TempDirGuard` test helper (`tests/common/mod.rs`, mode `0o700` on Unix) and refactored `tests/cli_runner_test.rs` and `tests/error_logger_test.rs` to eliminate `~/.local/state` writes and ensure panic-safe temporary directory cleanup.

### Added
- Integration test in `tests/cli_runner_test.rs` verifying that `--version` output matches `env!("CARGO_PKG_VERSION")`.

## [1.0.0] - 2026-11-20

### Added
- Initial stable release of `epub-translator-google` with Ports & Adapters (Hexagonal) architecture in Rust 2024 edition.
- Support for Google Antigravity (Gemini), Google Free (`gtx`), and Google Cloud Translation API v2 providers.
- Interactive TUI and strict `--headless` execution modes, `verify` OCF/XML 1.0 subcommand, and `history` telemetry subcommand (`table`, `json`, `csv`).
- SQLite WAL cache with ZSTD level-3 compression, adaptive XHTML partitioning, and two-pass XML 1.0 syntactic auto-healer.
- GitHub Actions CI, security audit, and static `musl` multi-architecture release workflows.
