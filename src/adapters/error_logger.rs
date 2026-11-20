use crate::domain::errors::{AppError, HttpErrorDetails, ProcessErrorDetails, TranslationError};
use std::backtrace::Backtrace;
use std::error::Error;
use std::fs::{DirBuilder, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Default)]
pub struct ExecutionContext {
    pub input_path: Option<PathBuf>,
    pub output_path: Option<PathBuf>,
    pub source_lang: Option<String>,
    pub target_lang: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub cache_db: Option<PathBuf>,
}

pub struct LogTimestamp {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub nanos: u32,
    pub local_iso: String,
    pub utc_iso: String,
    pub filename_base: String,
}

impl LogTimestamp {
    pub fn now() -> Self {
        let duration_since_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let secs = duration_since_epoch.as_secs();
        let nanos = duration_since_epoch.subsec_nanos();

        let days = (secs / 86400) as i64;
        let rem_secs = secs % 86400;
        let hour = (rem_secs / 3600) as u32;
        let minute = ((rem_secs % 3600) / 60) as u32;
        let second = (rem_secs % 60) as u32;

        let z = days + 719468;
        let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
        let doe = (z - era * 146097) as u32;
        let yoe = (doe - doe / 1465 + doe / 36524 - doe / 146096) / 365;
        let y = (yoe as i64) + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = (mp as i32) + (if mp < 10 { 3 } else { -9 });
        let year = (y + (if m <= 2 { 1 } else { 0 })) as i32;
        let month = m as u32;
        let day = d;

        let utc_iso = format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:09}Z",
            year, month, day, hour, minute, second, nanos
        );
        let local_iso = utc_iso.clone();

        let filename_base =
            format!("erro_{year:04}{month:02}{day:02}{hour:02}{minute:02}{second:02}{nanos:09}");

        Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
            nanos,
            local_iso,
            utc_iso,
            filename_base,
        }
    }
}

pub struct ErrorLogger;

impl ErrorLogger {
    fn current_user_discriminator() -> String {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if let Ok(meta) = std::fs::metadata("/proc/self") {
                return meta.uid().to_string();
            }
            if let Ok(uid) = std::env::var("UID") {
                let trimmed = uid.trim();
                if !trimmed.is_empty() {
                    return Self::sanitize_path_component(trimmed);
                }
            }
        }
        if let Ok(user) = std::env::var("USER").or_else(|_| std::env::var("USERNAME")) {
            let trimmed = user.trim();
            if !trimmed.is_empty() {
                return Self::sanitize_path_component(trimmed);
            }
        }
        format!("pid-{}", std::process::id())
    }

    fn sanitize_path_component(input: &str) -> String {
        let cleaned: String = input
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
            .collect();
        if cleaned.is_empty() {
            format!("pid-{}", std::process::id())
        } else {
            cleaned
        }
    }

    fn ensure_secure_dir(dir: &Path) -> Result<(), std::io::Error> {
        let mut builder = DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
            builder.mode(0o700);
            builder.create(dir)?;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        }
        #[cfg(not(unix))]
        {
            builder.create(dir)?;
        }
        Ok(())
    }

    pub fn resolve_logs_dir() -> PathBuf {
        Self::resolve_logs_dir_with(dirs::state_dir(), dirs::data_local_dir())
    }

    pub fn resolve_logs_dir_with(
        state_dir: Option<PathBuf>,
        data_local_dir: Option<PathBuf>,
    ) -> PathBuf {
        let app_subpath = Path::new("epub-translator-google").join("logs");
        if let Some(dir) = state_dir {
            return dir.join(&app_subpath);
        }
        if let Some(dir) = data_local_dir {
            return dir.join(&app_subpath);
        }
        let uid = Self::current_user_discriminator();
        let fallback_root = std::env::temp_dir().join(format!("epub-translator-google-{uid}"));
        let logs_dir = fallback_root.join("logs");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = Self::ensure_secure_dir(&logs_dir);
            let _ =
                std::fs::set_permissions(&fallback_root, std::fs::Permissions::from_mode(0o700));
            let _ = std::fs::set_permissions(&logs_dir, std::fs::Permissions::from_mode(0o700));
        }
        #[cfg(not(unix))]
        {
            let _ = Self::ensure_secure_dir(&logs_dir);
        }
        logs_dir
    }

    pub fn log_error(err: &AppError, ctx: &ExecutionContext) -> Result<PathBuf, std::io::Error> {
        let logs_dir = Self::resolve_logs_dir();
        Self::log_error_to_dir(&logs_dir, err, ctx)
    }

    pub fn log_error_to_dir(
        logs_dir: &Path,
        err: &AppError,
        ctx: &ExecutionContext,
    ) -> Result<PathBuf, std::io::Error> {
        let ts = LogTimestamp::now();
        let (mut file, file_path) = Self::create_log_file_atomic(logs_dir, &ts.filename_base)?;
        let backtrace = Backtrace::force_capture();
        let report = Self::generate_report(err, ctx, &ts, &backtrace);
        file.write_all(report.as_bytes())?;
        file.flush()?;
        Ok(file_path)
    }

    fn open_secure_new_file(path: &Path) -> Result<File, std::io::Error> {
        let mut opts = OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        opts.open(path)
    }

    pub fn create_log_file_atomic(
        base_dir: &Path,
        base_name: &str,
    ) -> Result<(File, PathBuf), std::io::Error> {
        Self::ensure_secure_dir(base_dir)?;
        let first_candidate = base_dir.join(format!("{base_name}.log"));
        match Self::open_secure_new_file(&first_candidate) {
            Ok(file) => return Ok((file, first_candidate)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }

        let mut counter = 1;
        loop {
            let next_candidate = base_dir.join(format!("{base_name}_{counter}.log"));
            match Self::open_secure_new_file(&next_candidate) {
                Ok(file) => return Ok((file, next_candidate)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    counter += 1;
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn is_sensitive_header(name: &str) -> bool {
        matches!(
            name.to_ascii_lowercase().as_str(),
            "set-cookie" | "authorization" | "x-goog-api-key" | "proxy-authorization" | "cookie"
        )
    }

    pub fn sanitize_terminal_text(input: &str) -> String {
        Self::sanitize_control_and_ansi_impl(input, false)
    }

    pub fn sanitize_log_multiline(input: &str) -> String {
        Self::sanitize_control_and_ansi_impl(input, true)
    }

    fn sanitize_control_and_ansi_impl(input: &str, allow_newlines_and_tabs: bool) -> String {
        let mut out = String::with_capacity(input.len());
        let mut chars = input.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\x1b' {
                match chars.peek().copied() {
                    Some('[') => {
                        chars.next();
                        for c in chars.by_ref() {
                            if ('@'..='~').contains(&c) {
                                break;
                            }
                        }
                    }
                    Some(']') | Some('P') | Some('X') | Some('^') | Some('_') => {
                        chars.next();
                        while let Some(c) = chars.next() {
                            if c == '\x07' || c == '\u{009c}' {
                                break;
                            }
                            if c == '\x1b' && chars.peek() == Some(&'\\') {
                                chars.next();
                                break;
                            }
                        }
                    }
                    Some('(' | ')' | '*' | '+' | '-' | '.' | '/') => {
                        chars.next();
                        chars.next();
                    }
                    Some(next_ch) if (' '..='~').contains(&next_ch) => {
                        chars.next();
                    }
                    _ => {}
                }
                continue;
            }
            if ch == '\u{009b}' {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
                continue;
            }
            if ch == '\u{009d}'
                || ch == '\u{0090}'
                || ch == '\u{0098}'
                || ch == '\u{009e}'
                || ch == '\u{009f}'
            {
                while let Some(c) = chars.next() {
                    if c == '\x07' || c == '\u{009c}' {
                        break;
                    }
                    if c == '\x1b' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
                continue;
            }
            if ch.is_control() {
                if allow_newlines_and_tabs && (ch == '\n' || ch == '\t') {
                    out.push(ch);
                }
                continue;
            }
            out.push(ch);
        }
        out
    }

    fn generate_report(
        err: &AppError,
        ctx: &ExecutionContext,
        ts: &LogTimestamp,
        backtrace: &Backtrace,
    ) -> String {
        let (chapter_info, inner_err, error_location) = match err {
            AppError::ChapterTranslation(boxed) => (
                Some((
                    boxed.chapter_index,
                    boxed.chapter_path.as_str(),
                    boxed.chapter_size,
                    boxed.chapter_preview_start.as_str(),
                    boxed.chapter_preview_end.as_str(),
                )),
                Some(&boxed.source),
                Some(boxed.location),
            ),
            AppError::Translation(t_err) => (None, Some(t_err), None),
            _ => (None, None, None),
        };

        let process_details = match inner_err {
            Some(TranslationError::ProcessDetailed(details)) => Some(details.as_ref()),
            _ => None,
        };

        let http_details = match inner_err {
            Some(TranslationError::HttpDetailed(details)) => Some(details.as_ref()),
            _ => None,
        };

        let resolved_location = error_location
            .or_else(|| process_details.map(|p| p.location))
            .map(|loc| loc.to_string())
            .unwrap_or_else(|| "src/main.rs:run".to_string());

        let error_type = match err {
            AppError::ChapterTranslation(_) => "AppError::ChapterTranslation",
            AppError::Translation(_) => "AppError::Translation",
            AppError::Epub(_) => "AppError::Epub",
            AppError::Cache(_) => "AppError::Cache",
            AppError::Config(_) => "AppError::Config",
            AppError::UserCancelled => "AppError::UserCancelled",
        };

        let (category, diagnosis, human_actions, ai_actions) =
            Self::diagnose_root_cause(err, process_details, http_details);

        let mut report = String::with_capacity(8192);

        report.push_str(
            "================================================================================\n",
        );
        report.push_str("RELATÓRIO DE ERRO FATAL - EPUB TRANSLATOR GOOGLE\n");
        report.push_str(
            "================================================================================\n",
        );
        report.push_str(&format!(
            "DATA/HORA:     {} ({})\n",
            ts.local_iso, ts.utc_iso
        ));
        report.push_str(&format!(
            "ARQUIVO/LINHA: {}\n",
            Self::sanitize_terminal_text(&resolved_location)
        ));
        report.push_str(&format!("TIPO DO ERRO:  {}\n", error_type));
        report.push_str(&format!(
            "O QUE HOUVE:   {}\n",
            Self::sanitize_terminal_text(&err.to_string())
        ));
        report.push_str(&format!(
            "POR QUE:       {}\n",
            Self::sanitize_terminal_text(&category)
        ));
        report.push_str(
            "================================================================================\n\n",
        );

        report.push_str("=== 1. CONTEXTO OPERACIONAL ===\n");
        let input_display = ctx
            .input_path
            .as_ref()
            .map(|p| Self::sanitize_terminal_text(&p.display().to_string()))
            .unwrap_or_else(|| "N/A".to_string());
        report.push_str(&format!("Arquivo EPUB de Entrada: {}\n", input_display));

        if let Some(ref p) = ctx.input_path
            && let Ok(metadata) = std::fs::metadata(p)
        {
            let size_bytes = metadata.len();
            let size_mb = size_bytes as f64 / (1024.0 * 1024.0);
            report.push_str(&format!(
                "Tamanho do Arquivo:      {} bytes ({:.2} MB)\n",
                size_bytes, size_mb
            ));
        }

        let output_display = ctx
            .output_path
            .as_ref()
            .map(|p| Self::sanitize_terminal_text(&p.display().to_string()))
            .unwrap_or_else(|| "N/A".to_string());
        report.push_str(&format!("Arquivo de Saída:        {}\n", output_display));

        if let Some((idx, path, size, _, _)) = chapter_info {
            report.push_str(&format!(
                "Capítulo em Processo:    Índice #{} - {}\n",
                idx,
                Self::sanitize_terminal_text(path)
            ));
            report.push_str(&format!(
                "Tamanho do Capítulo:     {} bytes ({:.2} KB)\n",
                size,
                size as f64 / 1024.0
            ));
        } else {
            report.push_str("Capítulo em Processo:    N/A (Nenhum capítulo em tradução ativa)\n");
        }

        report.push_str(&format!(
            "Idioma de Origem:        {}\n",
            Self::sanitize_terminal_text(ctx.source_lang.as_deref().unwrap_or("N/A"))
        ));
        report.push_str(&format!(
            "Idioma de Destino:       {}\n",
            Self::sanitize_terminal_text(ctx.target_lang.as_deref().unwrap_or("N/A"))
        ));
        report.push_str(&format!(
            "Provedor Selecionado:    {}\n",
            Self::sanitize_terminal_text(ctx.provider.as_deref().unwrap_or("N/A"))
        ));
        report.push_str(&format!(
            "Modelo Selecionado:      {}\n",
            Self::sanitize_terminal_text(ctx.model.as_deref().unwrap_or("[Padrão do Sistema]"))
        ));
        report.push_str(&format!(
            "Banco de Cache:          {}\n\n",
            ctx.cache_db
                .as_ref()
                .map(|p| Self::sanitize_terminal_text(&p.display().to_string()))
                .unwrap_or_else(|| "N/A".to_string())
        ));

        report.push_str("=== 2. CADEIA CAUSAL DE ERROS (ERROR CHAIN) ===\n");
        let mut chain = Vec::new();
        chain.push(Self::sanitize_terminal_text(&err.to_string()));
        let mut current: &dyn Error = err;
        while let Some(cause) = current.source() {
            chain.push(Self::sanitize_terminal_text(&cause.to_string()));
            current = cause;
        }
        for (depth, cause_msg) in chain.iter().enumerate() {
            if depth == 0 {
                report.push_str(&format!("  [Nível 0 - Erro Primário]: {}\n", cause_msg));
            } else {
                report.push_str(&format!("  [Nível {} - Causa]: {}\n", depth, cause_msg));
            }
        }
        report.push('\n');

        report.push_str("=== 3. STACK TRACE COMPLETA (BACKTRACE) ===\n");
        let sanitized_backtrace = Self::sanitize_log_multiline(&backtrace.to_string());
        report.push_str(&format!("{sanitized_backtrace}\n"));

        report.push_str("=== 4. SUBPROCESSO E SISTEMA OPERACIONAL ===\n");
        if let Some(proc) = process_details {
            report.push_str(&format!(
                "Comando Executado:       {}\n",
                Self::sanitize_terminal_text(&proc.command)
            ));
            let sanitized_args: Vec<String> = proc
                .args
                .iter()
                .map(|a| Self::sanitize_terminal_text(a))
                .collect();
            report.push_str(&format!("Argumentos Passados:     {:?}\n", sanitized_args));
            report.push_str(&format!(
                "Modo de I/O:             {}\n",
                Self::sanitize_terminal_text(&proc.io_mode)
            ));
            report.push_str(&format!(
                "Código de Saída (Exit):  {}\n",
                proc.exit_code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "N/A".to_string())
            ));
            report.push_str(&format!(
                "Código de Erro do SO:    {}\n",
                proc.os_error_code
                    .map(|c| format!("{c} ({})", Self::errno_name(c)))
                    .unwrap_or_else(|| "N/A".to_string())
            ));
            report.push_str(&format!(
                "Local de Invocação:      {}\n",
                Self::sanitize_terminal_text(&proc.location.to_string())
            ));
            report.push_str("STDOUT Capturado:\n");
            report.push_str("--------------------------------------------------------------------------------\n");
            let sanitized_stdout = Self::sanitize_log_multiline(&proc.stdout);
            if sanitized_stdout.is_empty() {
                report.push_str("(vazio)\n");
            } else {
                report.push_str(&sanitized_stdout);
                if !sanitized_stdout.ends_with('\n') {
                    report.push('\n');
                }
            }
            report.push_str("--------------------------------------------------------------------------------\n");
            report.push_str("STDERR Capturado:\n");
            report.push_str("--------------------------------------------------------------------------------\n");
            let sanitized_stderr = Self::sanitize_log_multiline(&proc.stderr);
            if sanitized_stderr.is_empty() {
                report.push_str("(vazio)\n");
            } else {
                report.push_str(&sanitized_stderr);
                if !sanitized_stderr.ends_with('\n') {
                    report.push('\n');
                }
            }
            report.push_str("--------------------------------------------------------------------------------\n\n");
        } else {
            report.push_str(
                "Nenhum subprocesso externo em falha direta registrado para esta ocorrência.\n\n",
            );
        }

        report.push_str("=== DETALHES DA REQUISIÇÃO HTTP (REQUEST & RESPONSE) ===\n");
        if let Some(http) = http_details {
            report.push_str(&format!(
                "Método e URL Exatos:     {} {}\n",
                Self::sanitize_terminal_text(&http.method),
                Self::sanitize_terminal_text(&http.url)
            ));
            report.push_str(&format!(
                "Código de Status:        {} {}\n\n",
                http.status_code,
                Self::http_status_reason(http.status_code)
            ));

            report.push_str("Headers da Requisição:\n");
            report.push_str("--------------------------------------------------------------------------------\n");
            if http.request_headers.is_empty() {
                report.push_str("(nenhum header registrado)\n");
            } else {
                Self::append_sanitized_headers(&mut report, &http.request_headers);
            }
            report.push_str("--------------------------------------------------------------------------------\n\n");

            report.push_str("Corpo da Requisição (Payload Enviado):\n");
            report.push_str("--------------------------------------------------------------------------------\n");
            let sanitized_req_body = Self::sanitize_log_multiline(&http.request_body);
            if sanitized_req_body.is_empty() {
                report.push_str("(vazio)\n");
            } else {
                report.push_str(&sanitized_req_body);
                if !sanitized_req_body.ends_with('\n') {
                    report.push('\n');
                }
            }
            report.push_str("--------------------------------------------------------------------------------\n\n");

            report.push_str("Headers da Resposta:\n");
            report.push_str("--------------------------------------------------------------------------------\n");
            if http.response_headers.is_empty() {
                report.push_str("(nenhum header retornado)\n");
            } else {
                Self::append_sanitized_headers(&mut report, &http.response_headers);
            }
            report.push_str("--------------------------------------------------------------------------------\n\n");

            report.push_str("Corpo da Resposta do Servidor:\n");
            report.push_str("--------------------------------------------------------------------------------\n");
            let sanitized_resp_body = Self::sanitize_log_multiline(&http.response_body);
            if sanitized_resp_body.is_empty() {
                report.push_str("(vazio)\n");
            } else {
                report.push_str(&sanitized_resp_body);
                if !sanitized_resp_body.ends_with('\n') {
                    report.push('\n');
                }
            }
            report.push_str("--------------------------------------------------------------------------------\n\n");
        } else {
            report.push_str(
                "Nenhuma requisição HTTP direta em falha registrada para esta ocorrência.\n\n",
            );
        }

        report.push_str("=== 5. SNIPPET DO CAPÍTULO ===\n");
        if let Some((_, _, _, preview_start, preview_end)) = chapter_info {
            report.push_str("--- Início do Capítulo (primeiras 15 linhas) ---\n");
            report.push_str(&Self::sanitize_log_multiline(preview_start));
            report.push_str("\n--- Fim do Capítulo (últimas 15 linhas) ---\n");
            report.push_str(&Self::sanitize_log_multiline(preview_end));
            report.push_str("\n\n");
        } else {
            report
                .push_str("Nenhum fragmento de capítulo em processamento no momento da falha.\n\n");
        }

        report.push_str("=== 6. GUIA DE RESOLUÇÃO / TROUBLESHOOTING ===\n");
        report.push_str(&format!(
            "Diagnóstico da Causa Raiz: {}\n\n",
            Self::sanitize_terminal_text(&diagnosis)
        ));
        report.push_str("Ações Recomendadas para Humanos:\n");
        for (i, action) in human_actions.iter().enumerate() {
            report.push_str(&format!(
                "  {}. {}\n",
                i + 1,
                Self::sanitize_terminal_text(action)
            ));
        }
        report.push_str("\nAções Recomendadas para Agentes e IAs:\n");
        for (i, action) in ai_actions.iter().enumerate() {
            report.push_str(&format!(
                "  {}. {}\n",
                i + 1,
                Self::sanitize_terminal_text(action)
            ));
        }
        report.push_str(
            "================================================================================\n",
        );

        report
    }

    fn append_sanitized_headers(report: &mut String, headers: &[(String, String)]) {
        for (k, v) in headers {
            let safe_k = Self::sanitize_terminal_text(k);
            let safe_v = if Self::is_sensitive_header(k) {
                "[REDACTED]".to_string()
            } else {
                Self::sanitize_terminal_text(v)
            };
            report.push_str(&format!("{safe_k}: {safe_v}\n"));
        }
    }

    fn http_status_reason(status: u16) -> &'static str {
        match status {
            200 => "OK",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            413 => "Payload Too Large",
            414 => "URI Too Long",
            429 => "Too Many Requests",
            500 => "Internal Server Error",
            502 => "Bad Gateway",
            503 => "Service Unavailable",
            504 => "Gateway Timeout",
            _ => "Unknown Status",
        }
    }

    pub fn summarize_http_body(body: &str) -> String {
        let trimmed = body.trim();
        if trimmed.is_empty() {
            return "Sem detalhes no corpo da resposta".to_string();
        }

        if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(msg) = json_val
                .pointer("/error/message")
                .and_then(serde_json::Value::as_str)
            {
                return Self::sanitize_terminal_text(msg);
            }
            if let Some(msg) = json_val.get("message").and_then(serde_json::Value::as_str) {
                return Self::sanitize_terminal_text(msg);
            }
        }

        if let Some(start_p) = trimmed.find("<p>")
            && let Some(end_p) = trimmed[start_p..].find("</p>")
        {
            let raw_p = &trimmed[start_p + 3..start_p + end_p];
            let cleaned = Self::clean_html_snippet(raw_p);
            if !cleaned.is_empty() {
                return Self::sanitize_terminal_text(&cleaned);
            }
        }

        if let Some(start_t) = trimmed.find("<title>")
            && let Some(end_t) = trimmed[start_t..].find("</title>")
        {
            let raw_t = &trimmed[start_t + 7..start_t + end_t];
            let cleaned = Self::clean_html_snippet(raw_t);
            if !cleaned.is_empty() {
                return Self::sanitize_terminal_text(&cleaned);
            }
        }

        let first_line =
            Self::sanitize_terminal_text(trimmed.lines().next().unwrap_or(trimmed).trim());
        let mut chars = first_line.chars();
        let prefix: String = chars.by_ref().take(120).collect();
        if chars.next().is_some() {
            format!("{prefix}...")
        } else {
            prefix
        }
    }

    fn clean_html_snippet(snippet: &str) -> String {
        let mut out = String::with_capacity(snippet.len());
        let mut in_tag = false;
        for ch in snippet.chars() {
            if ch == '<' {
                in_tag = true;
            } else if ch == '>' {
                in_tag = false;
            } else if !in_tag {
                out.push(ch);
            }
        }
        out.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn errno_name(code: i32) -> &'static str {
        match code {
            1 => "EPERM / Operation not permitted",
            2 => "ENOENT / No such file or directory",
            7 => "E2BIG / Argument list too long",
            12 => "ENOMEM / Out of memory",
            13 => "EACCES / Permission denied",
            16 => "EBUSY / Device or resource busy",
            24 => "EMFILE / Too many open files",
            28 => "ENOSPC / No space left on device",
            32 => "EPIPE / Broken pipe",
            110 => "ETIMEDOUT / Connection timed out",
            111 => "ECONNREFUSED / Connection refused",
            _ => "Unknown errno",
        }
    }

    fn diagnose_root_cause(
        err: &AppError,
        process_details: Option<&ProcessErrorDetails>,
        http_details: Option<&HttpErrorDetails>,
    ) -> (String, String, Vec<String>, Vec<String>) {
        if let Some(proc) = process_details {
            if proc.os_error_code == Some(7)
                || proc.message.contains("Argument list too long")
                || proc.message.contains("os error 7")
            {
                return (
                    "E2BIG / Argument List Overflow".to_string(),
                    "O comando atingiu o limite de argumentos do sistema operacional (ARG_MAX / execve limit) ao tentar passar o conteúdo de um capítulo volumoso diretamente via flag de linha de comando.".to_string(),
                    vec![
                        "Atualize o binário do tradutor utilizando ./install.sh para assegurar o envio de payloads via stdin não-bloqueante.".to_string(),
                        "Caso utilize scripts externos, nunca passe conteúdos de arquivos inteiros via flags como -p \"...\"; utilize pipes ou stdin streaming.".to_string(),
                    ],
                    vec![
                        "Inspecione src/adapters/translator/antigravity.rs e assegure que Command configure stdin(Stdio::piped()) e utilize AsyncWriteExt::write_all para enviar o prompt.".to_string(),
                        "Valide que nenhuma invocação de Command passe texto de tamanho não delimitado em .arg().".to_string(),
                    ],
                );
            }

            if let Some(exit_code) = proc.exit_code {
                return (
                    "External Process Execution Failure".to_string(),
                    format!(
                        "O comando '{}' retornou código de erro {} durante a execução.",
                        proc.command, exit_code
                    ),
                    vec![
                        "Verifique se o binário agy está autenticado e funcional no ambiente executando 'agy --help' e 'agy models'.".to_string(),
                        "Consulte a seção STDERR no relatório deste log para examinar mensagens de diagnóstico do processo.".to_string(),
                    ],
                    vec![
                        "Analise os argumentos passados e as mensagens emitidas em STDERR para identificar flags conflitantes ou erros de autenticação da CLI.".to_string(),
                        "Implemente sanitização e tratamento robusto de saídas de erro para a respectiva ferramenta CLI.".to_string(),
                    ],
                );
            }
        }

        if let Some(http) = http_details {
            let summary = Self::summarize_http_body(&http.response_body);
            let category = format!("HTTP {} - {}", http.status_code, summary);
            if http.status_code == 429 {
                let diagnosis = format!(
                    "A requisição HTTP {} para '{}' retornou código 429 (Too Many Requests). O WAF/CAPTCHA do Google impôs restrição temporária por IP devido a requisições consecutivas de chunks.",
                    http.method, http.url
                );
                let human_actions = vec![
                    "Aguarde alguns minutos para que o WAF do Google expire o bloqueio temporário por IP.".to_string(),
                    "Utilize o provedor '--provider antigravity' (ex: epub-translator-google -p antigravity), que opera com autenticação direta via CLI sem bloqueios por IP web.".to_string(),
                    "Como os capítulos anteriores já foram gravados no cache SQLite, o progresso está preservado e a tradução continuará de onde parou.".to_string(),
                ];
                let ai_actions = vec![
                    "Assegure cadência (pacing com jitter de 500ms a 1000ms) entre requisições consecutivas no adaptador Google Free.".to_string(),
                    "Recomende a utilização do provedor Antigravity para tradução ininterrupta de livros volumosos.".to_string(),
                ];
                return (category, diagnosis, human_actions, ai_actions);
            }
            let diagnosis = format!(
                "A requisição HTTP {} para '{}' retornou código {}: {}",
                http.method, http.url, http.status_code, summary
            );
            let human_actions = vec![
                "Verifique a conectividade com o provedor de tradução e a validade dos parâmetros enviados.".to_string(),
                "Consulte os detalhes da requisição e da resposta na seção 'DETALHES DA REQUISIÇÃO HTTP' deste relatório.".to_string(),
            ];
            let ai_actions = vec![
                "Inspecione os headers e o payload enviados em 'Corpo da Requisição' para identificar se excederam limites ou formato incorreto.".to_string(),
                "Assegure que o adaptador utilize particionamento seguro (chunking <= 1.800 caracteres) e codificação correta dos dados.".to_string(),
            ];
            return (category, diagnosis, human_actions, ai_actions);
        }

        match err {
            AppError::ChapterTranslation(boxed) => match &boxed.source {
                TranslationError::Serialization(msg) => (
                    "Serialization / Deserialization Failure".to_string(),
                    format!("A resposta retornada pelo provedor de tradução não pôde ser convertida no formato JSON esperado: {msg}"),
                    vec![
                        "Verifique se o modelo ou provedor selecionado está operando normalmente ou se retornou texto não-estruturado.".to_string(),
                        "Consulte o log detalhado para inspecionar a saída bruta retornada.".to_string(),
                    ],
                    vec![
                        "Inspecione a estrutura da resposta JSON e garanta tratamento adequado para campos opcionais ou formatos alternativos.".to_string(),
                    ],
                ),
                TranslationError::RateLimited(msg) => (
                    "Provider Rate Limit Exceeded / WAF IP Restriction".to_string(),
                    format!("O provedor de tradução retornou restrição por limite de requisições ou bloqueio temporário de IP por WAF: {msg}"),
                    vec![
                        "Aguarde alguns minutos para expiração do bloqueio temporário por IP do Google.".to_string(),
                        "Utilize o provedor '--provider antigravity' (ex: epub-translator-google -p antigravity), que opera com autenticação direta sem restrição por IP web.".to_string(),
                        "O progresso dos capítulos anteriores está preservado no cache SQLite e não precisará ser retraduzido.".to_string(),
                    ],
                    vec![
                        "Assegure cadência (pacing com jitter de 500ms a 1000ms) e semáforo unitário no adaptador Google Free.".to_string(),
                        "Recomende o chaveamento para o provedor Antigravity para volumes elevados de texto.".to_string(),
                    ],
                ),
                TranslationError::Authentication(msg) => (
                    "Authentication Failure".to_string(),
                    format!("Falha na autenticação com o provedor de tradução: {msg}"),
                    vec![
                        "Verifique suas credenciais de acesso ou execute o processo de login da ferramenta CLI.".to_string(),
                    ],
                    vec![
                        "Verifique chaves de API, variáveis de ambiente ou tokens de autenticação configurados no Composition Root.".to_string(),
                    ],
                ),
                _ => (
                    "Chapter Translation Failure".to_string(),
                    format!("Falha durante a tradução de um capítulo do livro: {}", boxed.source),
                    vec![
                        "Verifique os detalhes do erro do provedor no log e tente novamente.".to_string(),
                    ],
                    vec![
                        "Analise o tipo do erro TranslationError retornado para diagnosticar a causa do adaptador.".to_string(),
                    ],
                ),
            },
            AppError::Epub(epub_err) => (
                "EPUB Structure / Format Error".to_string(),
                format!("Falha ao ler, processar ou gravar o contêiner EPUB: {epub_err}"),
                vec![
                    "Certifique-se de que o arquivo fornecido é um EPUB válido (formato ZIP contendo META-INF/container.xml e OPF).".to_string(),
                    "Verifique se o arquivo não está corrompido ou protegido por DRM.".to_string(),
                ],
                vec![
                    "Valide a implementação do adaptador ZipEpubAdapter e os parsers de container.xml e package document OPF.".to_string(),
                ],
            ),
            AppError::Cache(cache_err) => (
                "Cache Storage Failure".to_string(),
                format!("Falha na persistência ou consulta do banco de cache SQLite: {cache_err}"),
                vec![
                    "Verifique se há espaço disponível em disco e permissões de escrita no caminho do banco de dados.".to_string(),
                ],
                vec![
                    "Valide a inicialização do esquema SQLite e as transações de leitura/escrita em SqliteCacheAdapter.".to_string(),
                ],
            ),
            AppError::Config(cfg_err) => (
                "Configuration Error".to_string(),
                format!("Parâmetros ou configurações inválidos: {cfg_err}"),
                vec![
                    "Revise os argumentos passados na linha de comando ou responda novamente às perguntas interativas.".to_string(),
                ],
                vec![
                    "Valide os parsers de CLIArgs e os prompts de InteractivePrompter.".to_string(),
                ],
            ),
            _ => (
                "Unhandled Application Error".to_string(),
                format!("Erro fatal não catalogado: {err}"),
                vec![
                    "Consulte o relatório completo neste arquivo de log para identificar a origem da falha.".to_string(),
                ],
                vec![
                    "Examine a seção STACK TRACE / BACKTRACE para rastrear a cadeia de chamadas até a origem.".to_string(),
                ],
            ),
        }
    }
}
