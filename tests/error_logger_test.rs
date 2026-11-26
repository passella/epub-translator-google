mod common;

use common::TempDirGuard;
use epub_translator_google::adapters::error_logger::{ErrorLogger, ExecutionContext, LogTimestamp};
use epub_translator_google::domain::errors::{
    AppError, ChapterTranslationError, HttpErrorDetails, ProcessErrorDetails, SourceLocation,
    TranslationError,
};
use std::fs;
use std::path::PathBuf;

#[test]
fn test_diagnose_root_cause_e2big_argument_list_too_long() {
    let proc = ProcessErrorDetails {
        command: "/usr/local/bin/agy".to_string(),
        args: vec!["--output-format".to_string(), "json".to_string()],
        io_mode: "stdin (streaming pipe)".to_string(),
        exit_code: None,
        os_error_code: Some(7),
        stdout: String::new(),
        stderr: String::new(),
        message: "Failed to execute agy CLI: Argument list too long (os error 7)".to_string(),
        location: SourceLocation::new("src/adapters/translator/antigravity.rs", 142, 9),
    };

    let translation_err = TranslationError::ProcessDetailed(Box::new(proc));
    let chapter_err = AppError::ChapterTranslation(Box::new(ChapterTranslationError {
        chapter_index: 3,
        chapter_path: "OEBPS/Text/chapter04.xhtml".to_string(),
        chapter_size: 512_000,
        chapter_preview_start: "<?xml version=\"1.0\" encoding=\"utf-8\"?>".to_string(),
        chapter_preview_end: "</body></html>".to_string(),
        location: SourceLocation::new("src/domain/services/translate_book.rs", 80, 17),
        source: translation_err,
    }));

    let _ctx = ExecutionContext {
        input_path: Some(PathBuf::from("/tmp/test_book.epub")),
        output_path: Some(PathBuf::from("/tmp/test_book_pt.epub")),
        source_lang: Some("en".to_string()),
        target_lang: Some("pt-BR".to_string()),
        provider: Some("antigravity".to_string()),
        model: Some("gemini-4-argon-medium".to_string()),
        cache_db: Some(PathBuf::from("translations_cache.db")),
    };

    let ts = LogTimestamp::now();
    assert!(ts.filename_base.starts_with("erro_"));
    assert_eq!(ts.filename_base.len(), 28);

    let temp_dir = TempDirGuard::new("test_err_logs");
    let (mut file, log_path) =
        ErrorLogger::create_log_file_atomic(temp_dir.path(), &ts.filename_base).unwrap();

    let backtrace = std::backtrace::Backtrace::force_capture();
    let report = format!("{chapter_err}\n{backtrace}");
    use std::io::Write;
    file.write_all(report.as_bytes()).unwrap();
    drop(file);

    assert!(log_path.exists());
}

#[test]
fn test_resolve_logs_dir_portable() {
    let logs_dir = ErrorLogger::resolve_logs_dir();
    assert!(logs_dir.ends_with("epub-translator-google/logs"));
}

#[test]
fn test_error_log_anti_collision_numbering() {
    let temp_dir = TempDirGuard::new("test_collision");
    let base_name = "erro_fixed_test";

    let (file1, path1) = ErrorLogger::create_log_file_atomic(temp_dir.path(), base_name).unwrap();
    assert_eq!(path1, temp_dir.path().join(format!("{base_name}.log")));
    drop(file1);

    let (file2, path2) = ErrorLogger::create_log_file_atomic(temp_dir.path(), base_name).unwrap();
    assert_eq!(path2, temp_dir.path().join(format!("{base_name}_1.log")));
    drop(file2);

    let (file3, path3) = ErrorLogger::create_log_file_atomic(temp_dir.path(), base_name).unwrap();
    assert_eq!(path3, temp_dir.path().join(format!("{base_name}_2.log")));
    drop(file3);

    assert!(path1.exists());
    assert!(path2.exists());
    assert!(path3.exists());
}

#[test]
fn test_error_report_structure_and_sections() {
    let proc = ProcessErrorDetails {
        command: "/usr/local/bin/agy".to_string(),
        args: vec!["--output-format".to_string(), "json".to_string()],
        io_mode: "stdin (streaming pipe)".to_string(),
        exit_code: Some(1),
        os_error_code: Some(7),
        stdout: "partial output".to_string(),
        stderr: "error: buffer overflow".to_string(),
        message: "agy CLI failed with status exit status 1: error: buffer overflow".to_string(),
        location: SourceLocation::new("src/adapters/translator/antigravity.rs", 180, 13),
    };

    let translation_err = TranslationError::ProcessDetailed(Box::new(proc));
    let chapter_err = AppError::ChapterTranslation(Box::new(ChapterTranslationError {
        chapter_index: 2,
        chapter_path: "OEBPS/Text/ch02.xhtml".to_string(),
        chapter_size: 4096,
        chapter_preview_start: "<p>Start of chapter</p>".to_string(),
        chapter_preview_end: "<p>End of chapter</p>".to_string(),
        location: SourceLocation::new("src/domain/services/translate_book.rs", 85, 21),
        source: translation_err,
    }));

    let ctx = ExecutionContext {
        input_path: Some(PathBuf::from("/tmp/sample_input.epub")),
        output_path: Some(PathBuf::from("/tmp/sample_output.epub")),
        source_lang: Some("en".to_string()),
        target_lang: Some("pt-BR".to_string()),
        provider: Some("antigravity".to_string()),
        model: Some("gemini-3.8-flash-high".to_string()),
        cache_db: Some(PathBuf::from("test_cache.db")),
    };

    let temp_logs_dir = TempDirGuard::new("test_err_report_structure");

    let generated_path =
        ErrorLogger::log_error_to_dir(temp_logs_dir.path(), &chapter_err, &ctx).unwrap();
    assert!(generated_path.exists());

    let content = fs::read_to_string(&generated_path).unwrap();

    assert!(content.contains("RELATÓRIO DE ERRO FATAL - EPUB TRANSLATOR GOOGLE"));
    assert!(content.contains("DATA/HORA:"));
    assert!(content.contains("ARQUIVO/LINHA:"));
    assert!(content.contains("src/domain/services/translate_book.rs:85:21"));
    assert!(content.contains("TIPO DO ERRO:  AppError::ChapterTranslation"));
    assert!(content.contains("O QUE HOUVE:"));
    assert!(content.contains("POR QUE:       E2BIG / Argument List Overflow"));

    assert!(content.contains("=== 1. CONTEXTO OPERACIONAL ==="));
    assert!(content.contains("Arquivo EPUB de Entrada: /tmp/sample_input.epub"));
    assert!(content.contains("Capítulo em Processo:    Índice #2 - OEBPS/Text/ch02.xhtml"));
    assert!(content.contains("Tamanho do Capítulo:     4096 bytes (4.00 KB)"));
    assert!(content.contains("Provedor Selecionado:    antigravity"));
    assert!(content.contains("Modelo Selecionado:      gemini-3.8-flash-high"));

    assert!(content.contains("=== 2. CADEIA CAUSAL DE ERROS (ERROR CHAIN) ==="));
    assert!(content.contains("[Nível 0 - Erro Primário]:"));
    assert!(content.contains("[Nível 1 - Causa]:"));

    assert!(content.contains("=== 3. STACK TRACE COMPLETA (BACKTRACE) ==="));

    assert!(content.contains("=== 4. SUBPROCESSO E SISTEMA OPERACIONAL ==="));
    assert!(content.contains("Comando Executado:       /usr/local/bin/agy"));
    assert!(content.contains("Modo de I/O:             stdin (streaming pipe)"));
    assert!(content.contains("Código de Saída (Exit):  1"));
    assert!(content.contains("Código de Erro do SO:    7 (E2BIG / Argument list too long)"));
    assert!(content.contains("STDOUT Capturado:"));
    assert!(content.contains("partial output"));
    assert!(content.contains("STDERR Capturado:"));
    assert!(content.contains("error: buffer overflow"));

    assert!(content.contains("=== 5. SNIPPET DO CAPÍTULO ==="));
    assert!(content.contains("<p>Start of chapter</p>"));
    assert!(content.contains("<p>End of chapter</p>"));

    assert!(content.contains("=== 6. GUIA DE RESOLUÇÃO / TROUBLESHOOTING ==="));
    assert!(content.contains("Ações Recomendadas para Humanos:"));
    assert!(content.contains("Ações Recomendadas para Agentes e IAs:"));
}

#[test]
fn test_error_report_http_detailed_logging() {
    let http = HttpErrorDetails {
        url: "https://translate.googleapis.com/translate_a/single".to_string(),
        method: "POST".to_string(),
        request_headers: vec![
            ("User-Agent".to_string(), "Mozilla/5.0 Chrome/133".to_string()),
            (
                "Content-Type".to_string(),
                "application/x-www-form-urlencoded".to_string(),
            ),
        ],
        request_body: "client=gtx&sl=auto&tl=pt-BR&dt=t&q=hello".to_string(),
        status_code: 400,
        response_headers: vec![
            ("content-type".to_string(), "text/html; charset=UTF-8".to_string()),
            ("date".to_string(), "Mon, 10 Nov 2026 11:05:16 GMT".to_string()),
        ],
        response_body: "<title>Error 400 (Bad Request)!!1</title><p>Your client has issued a malformed or illegal request.</p>".to_string(),
        message: "Google Free endpoint failed with HTTP 400 Bad Request".to_string(),
    };

    let translation_err = TranslationError::HttpDetailed(Box::new(http));
    let chapter_err = AppError::ChapterTranslation(Box::new(ChapterTranslationError {
        chapter_index: 5,
        chapter_path: "EPUB/ch005_contents.xhtml".to_string(),
        chapter_size: 38059,
        chapter_preview_start: "<?xml version='1.0' encoding='utf-8'?>".to_string(),
        chapter_preview_end: "</body></html>".to_string(),
        location: SourceLocation::new("src/domain/services/translate_book.rs", 90, 43),
        source: translation_err,
    }));

    let ctx = ExecutionContext {
        input_path: Some(PathBuf::from("/downloads/Go in Action.epub")),
        output_path: Some(PathBuf::from("/downloads/Go in Action-pt_br.epub")),
        source_lang: Some("auto".to_string()),
        target_lang: Some("pt-BR".to_string()),
        provider: Some("google-free".to_string()),
        model: None,
        cache_db: Some(PathBuf::from("translations_cache.db")),
    };

    let temp_logs_dir = TempDirGuard::new("test_err_http_detailed");

    let generated_path =
        ErrorLogger::log_error_to_dir(temp_logs_dir.path(), &chapter_err, &ctx).unwrap();
    assert!(generated_path.exists());

    let content = fs::read_to_string(&generated_path).unwrap();

    assert!(content.contains("RELATÓRIO DE ERRO FATAL - EPUB TRANSLATOR GOOGLE"));
    assert!(content.contains(
        "POR QUE:       HTTP 400 - Your client has issued a malformed or illegal request."
    ));
    assert!(content.contains("Capítulo em Processo:    Índice #5 - EPUB/ch005_contents.xhtml"));
    assert!(content.contains("Tamanho do Capítulo:     38059 bytes (37.17 KB)"));

    assert!(content.contains("=== DETALHES DA REQUISIÇÃO HTTP (REQUEST & RESPONSE) ==="));
    assert!(content.contains(
        "Método e URL Exatos:     POST https://translate.googleapis.com/translate_a/single"
    ));
    assert!(content.contains("Código de Status:        400 Bad Request"));
    assert!(content.contains("Content-Type: application/x-www-form-urlencoded"));
    assert!(content.contains("client=gtx&sl=auto&tl=pt-BR&dt=t&q=hello"));
    assert!(content.contains("content-type: text/html; charset=UTF-8"));
    assert!(content.contains("<p>Your client has issued a malformed or illegal request.</p>"));

    assert!(content.contains("=== 6. GUIA DE RESOLUÇÃO / TROUBLESHOOTING ==="));
}

#[test]
fn test_error_report_http_429_waf_ip_restriction() {
    let http = HttpErrorDetails {
        url: "https://translate.googleapis.com/translate_a/single".to_string(),
        method: "POST".to_string(),
        request_headers: vec![(
            "User-Agent".to_string(),
            "Mozilla/5.0 Chrome/133".to_string(),
        )],
        request_body: "client=gtx&sl=auto&tl=pt-BR&dt=t&q=hello".to_string(),
        status_code: 429,
        response_headers: vec![(
            "content-type".to_string(),
            "text/html; charset=UTF-8".to_string(),
        )],
        response_body: "<title>Error 429 (Too Many Requests)</title><p>Rate limit exceeded</p>"
            .to_string(),
        message: "Google Free endpoint failed with HTTP 429 Too Many Requests".to_string(),
    };

    let translation_err = TranslationError::HttpDetailed(Box::new(http));
    let chapter_err = AppError::ChapterTranslation(Box::new(ChapterTranslationError {
        chapter_index: 14,
        chapter_path: "EPUB/ch014.xhtml".to_string(),
        chapter_size: 25000,
        chapter_preview_start: "<div>".to_string(),
        chapter_preview_end: "</div>".to_string(),
        location: SourceLocation::new("src/domain/services/translate_book.rs", 90, 43),
        source: translation_err,
    }));

    let ctx = ExecutionContext {
        input_path: Some(PathBuf::from("/downloads/book.epub")),
        output_path: Some(PathBuf::from("/downloads/book-pt.epub")),
        source_lang: Some("en".to_string()),
        target_lang: Some("pt-BR".to_string()),
        provider: Some("google-free".to_string()),
        model: None,
        cache_db: Some(PathBuf::from("translations_cache.db")),
    };

    let temp_logs_dir = TempDirGuard::new("test_err_http_429");

    let generated_path =
        ErrorLogger::log_error_to_dir(temp_logs_dir.path(), &chapter_err, &ctx).unwrap();
    assert!(generated_path.exists());

    let content = fs::read_to_string(&generated_path).unwrap();
    assert!(content.contains("HTTP 429 - Rate limit exceeded"));
    assert!(content.contains("WAF/CAPTCHA do Google impôs restrição temporária por IP"));
    assert!(content.contains("--provider antigravity"));
}

#[test]
fn test_error_report_rate_limited_translation_error() {
    let translation_err = TranslationError::RateLimited(
        "Google Free endpoint returned HTTP 429 (Too Many Requests). WAF IP restriction"
            .to_string(),
    );
    let chapter_err = AppError::ChapterTranslation(Box::new(ChapterTranslationError {
        chapter_index: 14,
        chapter_path: "EPUB/ch014.xhtml".to_string(),
        chapter_size: 12000,
        chapter_preview_start: "<p>Text</p>".to_string(),
        chapter_preview_end: "</p>".to_string(),
        location: SourceLocation::new("src/domain/services/translate_book.rs", 90, 43),
        source: translation_err,
    }));

    let ctx = ExecutionContext {
        input_path: Some(PathBuf::from("/downloads/book.epub")),
        output_path: Some(PathBuf::from("/downloads/book-pt.epub")),
        source_lang: Some("en".to_string()),
        target_lang: Some("pt-BR".to_string()),
        provider: Some("google-free".to_string()),
        model: None,
        cache_db: Some(PathBuf::from("translations_cache.db")),
    };

    let temp_logs_dir = TempDirGuard::new("test_err_rate_limited");

    let generated_path =
        ErrorLogger::log_error_to_dir(temp_logs_dir.path(), &chapter_err, &ctx).unwrap();
    assert!(generated_path.exists());

    let content = fs::read_to_string(&generated_path).unwrap();
    assert!(content.contains("Provider Rate Limit Exceeded / WAF IP Restriction"));
    assert!(content.contains("--provider antigravity"));
}

#[test]
fn test_log_timestamp_validity_and_iso_formatting() {
    let ts = LogTimestamp::now();
    assert!(ts.year >= 2025);
    assert!((1..=12).contains(&ts.month));
    assert!((1..=31).contains(&ts.day));
    assert!(ts.hour <= 23);
    assert!(ts.minute <= 59);
    assert!(ts.second <= 59);
    assert!(ts.local_iso.contains('T'));
    assert!(ts.utc_iso.ends_with('Z'));
    assert_eq!(ts.filename_base.len(), 28);
    assert!(ts.filename_base.starts_with("erro_"));
}

#[test]
fn test_source_location_display_path_strips_absolute_machine_paths() {
    let loc_abs = SourceLocation::new(
        "/home/developer/dev/projetos/epub-translator-google/src/domain/errors.rs",
        12,
        5,
    );
    assert_eq!(loc_abs.display_path(), "src/domain/errors.rs");
    assert_eq!(format!("{loc_abs}"), "src/domain/errors.rs:12:5");

    let loc_tests = SourceLocation::new("/var/build/runner/tests/error_logger_test.rs", 42, 1);
    assert_eq!(loc_tests.display_path(), "tests/error_logger_test.rs");
    assert_eq!(format!("{loc_tests}"), "tests/error_logger_test.rs:42:1");

    let loc_rel = SourceLocation::new("src/main.rs", 1, 1);
    assert_eq!(loc_rel.display_path(), "src/main.rs");
    assert_eq!(format!("{loc_rel}"), "src/main.rs:1:1");
}
