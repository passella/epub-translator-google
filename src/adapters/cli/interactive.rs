use crate::adapters::translator::AntigravityTranslatorAdapter;
use crate::domain::entities::{Language, TranslationProviderKind};
use crate::domain::errors::AppError;
use crate::domain::ports::ModelDiscoveryPort;
use inquire::{InquireError, Password, Select, Text};
use std::path::{Path, PathBuf};

pub struct InteractivePrompter;

impl InteractivePrompter {
    pub fn prompt_input_path(provided: Option<PathBuf>) -> Result<PathBuf, AppError> {
        if let Some(path) = provided {
            if path.exists() {
                return Ok(path);
            }
            eprintln!("Arquivo informado não encontrado: {}", path.display());
        }

        let input_str = Text::new("Caminho do arquivo EPUB de entrada:")
            .prompt()
            .map_err(Self::map_inquire_error)?;

        let path = PathBuf::from(input_str.trim());
        if !path.exists() {
            return Err(AppError::Config(format!(
                "Arquivo não encontrado: {}",
                path.display()
            )));
        }

        Ok(path)
    }

    pub fn prompt_source_language(provided: Option<String>) -> Result<Language, AppError> {
        if let Some(src) = provided {
            return Ok(Language::new(src));
        }

        let options = vec![
            "auto (Detecção Automática)",
            "en (Inglês)",
            "es (Espanhol)",
            "fr (Francês)",
            "de (Alemão)",
            "Outro...",
        ];

        let selection = Select::new("Idioma de origem:", options)
            .prompt()
            .map_err(Self::map_inquire_error)?;

        match selection {
            "auto (Detecção Automática)" => Ok(Language::auto()),
            "en (Inglês)" => Ok(Language::new("en")),
            "es (Espanhol)" => Ok(Language::new("es")),
            "fr (Francês)" => Ok(Language::new("fr")),
            "de (Alemão)" => Ok(Language::new("de")),
            _ => {
                let custom = Text::new("Informe o código do idioma de origem (ex: it, ja, ru):")
                    .prompt()
                    .map_err(Self::map_inquire_error)?;
                Ok(Language::new(custom.trim()))
            }
        }
    }

    pub fn prompt_target_language(provided: Option<String>) -> Result<Language, AppError> {
        if let Some(tgt) = provided {
            return Ok(Language::new(tgt));
        }

        let options = vec![
            "pt-BR (Português Brasileiro)",
            "en (Inglês)",
            "es (Espanhol)",
            "fr (Francês)",
            "de (Alemão)",
            "Outro...",
        ];

        let selection = Select::new("Idioma de destino:", options)
            .prompt()
            .map_err(Self::map_inquire_error)?;

        match selection {
            "pt-BR (Português Brasileiro)" => Ok(Language::new("pt-BR")),
            "en (Inglês)" => Ok(Language::new("en")),
            "es (Espanhol)" => Ok(Language::new("es")),
            "fr (Francês)" => Ok(Language::new("fr")),
            "de (Alemão)" => Ok(Language::new("de")),
            _ => {
                let custom = Text::new("Informe o código do idioma de destino (ex: pt, it, ja):")
                    .prompt()
                    .map_err(Self::map_inquire_error)?;
                Ok(Language::new(custom.trim()))
            }
        }
    }

    pub fn prompt_provider(provided: Option<String>) -> Result<TranslationProviderKind, AppError> {
        if let Some(prov_str) = provided {
            return prov_str
                .parse::<TranslationProviderKind>()
                .map_err(AppError::Config);
        }

        let options = vec![
            "Google Antigravity (agy / Gemini - Máxima Fidelidade Literária)",
            "Google Free Endpoint (gtx - Gratuito / Sem Chaves)",
            "Google Cloud Translation API (Oficial / Chave de API)",
        ];

        let selection = Select::new("Provedor para tradução:", options)
            .prompt()
            .map_err(Self::map_inquire_error)?;

        match selection {
            "Google Antigravity (agy / Gemini - Máxima Fidelidade Literária)" => {
                Ok(TranslationProviderKind::Antigravity)
            }
            "Google Free Endpoint (gtx - Gratuito / Sem Chaves)" => {
                Ok(TranslationProviderKind::GoogleFree)
            }
            _ => Ok(TranslationProviderKind::GoogleCloud),
        }
    }

    pub async fn prompt_model(provided: Option<String>) -> Result<Option<String>, AppError> {
        let adapter = AntigravityTranslatorAdapter::default();
        Self::prompt_model_with_discovery(provided, &adapter).await
    }

    pub async fn prompt_model_with_discovery(
        provided: Option<String>,
        discovery: &dyn ModelDiscoveryPort,
    ) -> Result<Option<String>, AppError> {
        if let Some(m) = provided {
            let trimmed = m.trim().to_string();
            if trimmed.is_empty()
                || trimmed.eq_ignore_ascii_case("default")
                || trimmed.contains("Padrão")
            {
                return Ok(None);
            }
            return Ok(Some(trimmed));
        }

        let models = discovery.fetch_available_models().await;

        let default_option = "[Padrão do Sistema] (Recomendado)";
        let manual_option = "Outro (Digitar manualmente)...";

        let mut options = Vec::with_capacity(models.len() + 2);
        options.push(default_option.to_string());
        for (id, desc) in &models {
            if desc.is_empty() || desc == id {
                options.push(id.clone());
            } else {
                options.push(format!("{id} - {desc}"));
            }
        }
        options.push(manual_option.to_string());

        let selection = Select::new("Modelo do Google Antigravity:", options)
            .prompt()
            .map_err(Self::map_inquire_error)?;

        if selection == default_option {
            Ok(None)
        } else if selection == manual_option {
            let custom = Text::new("Informe o nome do modelo (ex: gemini-4-argon-high):")
                .prompt()
                .map_err(Self::map_inquire_error)?;
            let trimmed = custom.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        } else {
            let model_id = selection.split_whitespace().next().unwrap_or(&selection);
            Ok(Some(model_id.to_string()))
        }
    }

    pub fn compute_default_output(input: &Path, target: &Language) -> PathBuf {
        let stem = input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("livro");
        let lang_suffix = target.as_str().to_lowercase().replace('-', "_");
        let file_name = format!("{stem}-{lang_suffix}.epub");
        input.with_file_name(file_name)
    }

    pub fn prompt_output_path(
        provided: Option<PathBuf>,
        input: &Path,
        target: &Language,
    ) -> Result<PathBuf, AppError> {
        if let Some(path) = provided {
            return Ok(path);
        }

        let default_path = Self::compute_default_output(input, target);
        let default_str = default_path.to_string_lossy().to_string();

        let output_str = Text::new("Arquivo de destino:")
            .with_default(&default_str)
            .prompt()
            .map_err(Self::map_inquire_error)?;

        Ok(PathBuf::from(output_str.trim()))
    }

    pub fn prompt_google_cloud_api_key() -> Result<String, AppError> {
        let key = Password::new("Chave de API do Google Cloud Translation:")
            .without_confirmation()
            .prompt()
            .map_err(Self::map_inquire_error)?;
        Ok(key.trim().to_string())
    }

    fn map_inquire_error(e: InquireError) -> AppError {
        match e {
            InquireError::OperationCanceled | InquireError::OperationInterrupted => {
                AppError::UserCancelled
            }
            other => AppError::Config(format!("Prompt error: {other}")),
        }
    }
}
