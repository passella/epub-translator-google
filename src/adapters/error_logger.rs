use crate::domain::errors::{AppError, HttpErrorDetails, ProcessErrorDetails, TranslationError};
use std::backtrace::Backtrace;
use std::error::Error;
use std::fs::{File, OpenOptions, create_dir_all};
use std::io::Write;
use std::path::{Path, PathBuf};

#[repr(C)]
struct Tm {
    tm_sec: std::ffi::c_int,
    tm_min: std::ffi::c_int,
    tm_hour: std::ffi::c_int,
    tm_mday: std::ffi::c_int,
    tm_mon: std::ffi::c_int,
    tm_year: std::ffi::c_int,
    tm_wday: std::ffi::c_int,
    tm_yday: std::ffi::c_int,
    tm_isdst: std::ffi::c_int,
    tm_gmtoff: std::ffi::c_long,
    tm_zone: *const std::ffi::c_char,
}

unsafe extern "C" {
    fn localtime_r(timep: *const i64, result: *mut Tm) -> *mut Tm;
    fn gmtime_r(timep: *const i64, result: *mut Tm) -> *mut Tm;
}

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
        let now = std::time::SystemTime::now();
        let duration = now
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let secs = duration.as_secs() as i64;
        let nanos = duration.subsec_nanos();

        let mut tm_local = std::mem::MaybeUninit::<Tm>::zeroed();
        let local = unsafe {
            localtime_r(&secs, tm_local.as_mut_ptr());
            tm_local.assume_init()
        };

        let year = local.tm_year + 1900;
        let month = (local.tm_mon + 1) as u32;
        let day = local.tm_mday as u32;
        let hour = local.tm_hour as u32;
        let minute = local.tm_min as u32;
        let second = local.tm_sec as u32;
        let gmtoff = local.tm_gmtoff;

        let offset_sign = if gmtoff >= 0 { '+' } else { '-' };
        let abs_offset = gmtoff.unsigned_abs();
        let offset_h = abs_offset / 3600;
        let offset_m = (abs_offset % 3600) / 60;

        let local_iso = format!(
            "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{nanos:09}{offset_sign}{offset_h:02}:{offset_m:02}"
        );

        let mut tm_utc = std::mem::MaybeUninit::<Tm>::zeroed();
        let utc = unsafe {
            gmtime_r(&secs, tm_utc.as_mut_ptr());
            tm_utc.assume_init()
        };
        let utc_year = utc.tm_year + 1900;
        let utc_month = (utc.tm_mon + 1) as u32;
        let utc_day = utc.tm_mday as u32;
        let utc_hour = utc.tm_hour as u32;
        let utc_minute = utc.tm_min as u32;
        let utc_second = utc.tm_sec as u32;

        let utc_iso = format!(
            "{utc_year:04}-{utc_month:02}-{utc_day:02}T{utc_hour:02}:{utc_minute:02}:{utc_second:02}.{nanos:09}Z"
        );

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
    pub fn resolve_logs_dir() -> PathBuf {
        let app_subpath = Path::new("epub-translator-google").join("logs");
        if let Some(state_dir) = dirs::state_dir() {
            return state_dir.join(&app_subpath);
        }
        if let Some(data_local_dir) = dirs::data_local_dir() {
            return data_local_dir.join(&app_subpath);
        }
        std::env::temp_dir().join(app_subpath)
    }

    pub fn log_error(err: &AppError, ctx: &ExecutionContext) -> Result<PathBuf, std::io::Error> {
        let ts = LogTimestamp::now();
        let logs_dir = Self::resolve_logs_dir();
        let (mut file, file_path) = Self::create_log_file_atomic(&logs_dir, &ts.filename_base)?;
        let backtrace = Backtrace::force_capture();
        let report = Self::generate_report(err, ctx, &ts, &backtrace);
        file.write_all(report.as_bytes())?;
        file.flush()?;
        Ok(file_path)
    }

    pub fn create_log_file_atomic(
        base_dir: &Path,
        base_name: &str,
    ) -> Result<(File, PathBuf), std::io::Error> {
        create_dir_all(base_dir)?;
        let first_candidate = base_dir.join(format!("{base_name}.log"));
        if let Ok(file) = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&first_candidate)
        {
            return Ok((file, first_candidate));
        }

        let mut counter = 1;
        loop {
            let next_candidate = base_dir.join(format!("{base_name}_{counter}.log"));
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&next_candidate)
            {
                Ok(file) => return Ok((file, next_candidate)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    counter += 1;
                }
                Err(e) => return Err(e),
            }
        }
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
        report.push_str(&format!("ARQUIVO/LINHA: {}\n", resolved_location));
        report.push_str(&format!("TIPO DO ERRO:  {}\n", error_type));
        report.push_str(&format!("O QUE HOUVE:   {}\n", err));
        report.push_str(&format!("POR QUE:       {}\n", category));
        report.push_str(
            "================================================================================\n\n",
        );

        report.push_str("=== 1. CONTEXTO OPERACIONAL ===\n");
        let input_display = ctx
            .input_path
            .as_ref()
            .map(|p| p.display().to_string())
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
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "N/A".to_string());
        report.push_str(&format!("Arquivo de Saída:        {}\n", output_display));

        if let Some((idx, path, size, _, _)) = chapter_info {
            report.push_str(&format!(
                "Capítulo em Processo:    Índice #{} - {}\n",
                idx, path
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
            ctx.source_lang.as_deref().unwrap_or("N/A")
        ));
        report.push_str(&format!(
            "Idioma de Destino:       {}\n",
            ctx.target_lang.as_deref().unwrap_or("N/A")
        ));
        report.push_str(&format!(
            "Provedor Selecionado:    {}\n",
            ctx.provider.as_deref().unwrap_or("N/A")
        ));
        report.push_str(&format!(
            "Modelo Selecionado:      {}\n",
            ctx.model.as_deref().unwrap_or("[Padrão do Sistema]")
        ));
        report.push_str(&format!(
            "Banco de Cache:          {}\n\n",
            ctx.cache_db
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "N/A".to_string())
        ));

        report.push_str("=== 2. CADEIA CAUSAL DE ERROS (ERROR CHAIN) ===\n");
        let mut chain = Vec::new();
        chain.push(err.to_string());
        let mut current: &dyn Error = err;
        while let Some(cause) = current.source() {
            chain.push(cause.to_string());
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
        report.push_str(&format!("{backtrace}\n"));

        report.push_str("=== 4. SUBPROCESSO E SISTEMA OPERACIONAL ===\n");
        if let Some(proc) = process_details {
            report.push_str(&format!("Comando Executado:       {}\n", proc.command));
            report.push_str(&format!("Argumentos Passados:     {:?}\n", proc.args));
            report.push_str(&format!("Modo de I/O:             {}\n", proc.io_mode));
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
            report.push_str(&format!("Local de Invocação:      {}\n", proc.location));
            report.push_str("STDOUT Capturado:\n");
            report.push_str("--------------------------------------------------------------------------------\n");
            if proc.stdout.is_empty() {
                report.push_str("(vazio)\n");
            } else {
                report.push_str(&proc.stdout);
                if !proc.stdout.ends_with('\n') {
                    report.push('\n');
                }
            }
            report.push_str("--------------------------------------------------------------------------------\n");
            report.push_str("STDERR Capturado:\n");
            report.push_str("--------------------------------------------------------------------------------\n");
            if proc.stderr.is_empty() {
                report.push_str("(vazio)\n");
            } else {
                report.push_str(&proc.stderr);
                if !proc.stderr.ends_with('\n') {
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
                http.method, http.url
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
                for (k, v) in &http.request_headers {
                    report.push_str(&format!("{k}: {v}\n"));
                }
            }
            report.push_str("--------------------------------------------------------------------------------\n\n");

            report.push_str("Corpo da Requisição (Payload Enviado):\n");
            report.push_str("--------------------------------------------------------------------------------\n");
            if http.request_body.is_empty() {
                report.push_str("(vazio)\n");
            } else {
                report.push_str(&http.request_body);
                if !http.request_body.ends_with('\n') {
                    report.push('\n');
                }
            }
            report.push_str("--------------------------------------------------------------------------------\n\n");

            report.push_str("Headers da Resposta:\n");
            report.push_str("--------------------------------------------------------------------------------\n");
            if http.response_headers.is_empty() {
                report.push_str("(nenhum header retornado)\n");
            } else {
                for (k, v) in &http.response_headers {
                    report.push_str(&format!("{k}: {v}\n"));
                }
            }
            report.push_str("--------------------------------------------------------------------------------\n\n");

            report.push_str("Corpo da Resposta do Servidor:\n");
            report.push_str("--------------------------------------------------------------------------------\n");
            if http.response_body.is_empty() {
                report.push_str("(vazio)\n");
            } else {
                report.push_str(&http.response_body);
                if !http.response_body.ends_with('\n') {
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
            report.push_str(preview_start);
            report.push_str("\n--- Fim do Capítulo (últimas 15 linhas) ---\n");
            report.push_str(preview_end);
            report.push_str("\n\n");
        } else {
            report
                .push_str("Nenhum fragmento de capítulo em processamento no momento da falha.\n\n");
        }

        report.push_str("=== 6. GUIA DE RESOLUÇÃO / TROUBLESHOOTING ===\n");
        report.push_str(&format!("Diagnóstico da Causa Raiz: {}\n\n", diagnosis));
        report.push_str("Ações Recomendadas para Humanos:\n");
        for (i, action) in human_actions.iter().enumerate() {
            report.push_str(&format!("  {}. {}\n", i + 1, action));
        }
        report.push_str("\nAções Recomendadas para Agentes e IAs:\n");
        for (i, action) in ai_actions.iter().enumerate() {
            report.push_str(&format!("  {}. {}\n", i + 1, action));
        }
        report.push_str(
            "================================================================================\n",
        );

        report
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

    fn summarize_http_body(body: &str) -> String {
        let trimmed = body.trim();
        if trimmed.is_empty() {
            return "Sem detalhes no corpo da resposta".to_string();
        }

        if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(msg) = json_val
                .pointer("/error/message")
                .and_then(serde_json::Value::as_str)
            {
                return msg.to_string();
            }
            if let Some(msg) = json_val.get("message").and_then(serde_json::Value::as_str) {
                return msg.to_string();
            }
        }

        if let Some(start_p) = trimmed.find("<p>")
            && let Some(end_p) = trimmed[start_p..].find("</p>")
        {
            let raw_p = &trimmed[start_p + 3..start_p + end_p];
            let cleaned = Self::clean_html_snippet(raw_p);
            if !cleaned.is_empty() {
                return cleaned;
            }
        }

        if let Some(start_t) = trimmed.find("<title>")
            && let Some(end_t) = trimmed[start_t..].find("</title>")
        {
            let raw_t = &trimmed[start_t + 7..start_t + end_t];
            let cleaned = Self::clean_html_snippet(raw_t);
            if !cleaned.is_empty() {
                return cleaned;
            }
        }

        let first_line = trimmed.lines().next().unwrap_or(trimmed).trim();
        if first_line.len() > 120 {
            format!("{}...", &first_line[..120])
        } else {
            first_line.to_string()
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
