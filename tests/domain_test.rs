use epub_translator_google::domain::entities::{
    EpubBook, EpubChapter, EpubMetadata, Language, TranslationProviderKind, TranslationRequest,
    TranslationResult,
};
use std::str::FromStr;

#[test]
fn test_language_value_object() {
    let lang = Language::new("pt-BR");
    assert_eq!(lang.as_str(), "pt-BR");
    assert_eq!(lang.to_string(), "pt-BR");
    assert!(!lang.is_auto());

    let auto_lang = Language::auto();
    assert_eq!(auto_lang.as_str(), "auto");
    assert!(auto_lang.is_auto());

    let from_str = Language::from("en");
    assert_eq!(from_str.as_str(), "en");
}

#[test]
fn test_translation_provider_kind_parsing() {
    assert_eq!(
        TranslationProviderKind::from_str("antigravity").unwrap(),
        TranslationProviderKind::Antigravity
    );
    assert_eq!(
        TranslationProviderKind::from_str("agy").unwrap(),
        TranslationProviderKind::Antigravity
    );
    assert_eq!(
        TranslationProviderKind::from_str("google-cloud").unwrap(),
        TranslationProviderKind::GoogleCloud
    );
    assert_eq!(
        TranslationProviderKind::from_str("gcp").unwrap(),
        TranslationProviderKind::GoogleCloud
    );
    assert_eq!(
        TranslationProviderKind::from_str("google-free").unwrap(),
        TranslationProviderKind::GoogleFree
    );
    assert_eq!(
        TranslationProviderKind::from_str("gtx").unwrap(),
        TranslationProviderKind::GoogleFree
    );
    assert!(TranslationProviderKind::from_str("unknown-provider").is_err());
}

#[test]
fn test_translation_request_and_result() {
    let req = TranslationRequest {
        text: "Hello".to_string(),
        source_lang: Language::new("en"),
        target_lang: Language::new("pt-BR"),
        is_html: false,
    };
    assert_eq!(req.text, "Hello");

    let res = TranslationResult {
        translated_text: "Olá".to_string(),
        detected_source: Some(Language::new("en")),
    };
    assert_eq!(res.translated_text, "Olá");
    assert_eq!(res.detected_source.unwrap().as_str(), "en");
}

#[test]
fn test_epub_metadata_and_book_structures() {
    let meta = EpubMetadata {
        title: Some("Livro de Teste".to_string()),
        language: Some(Language::new("pt-BR")),
        creator: Some("Autor".to_string()),
        identifier: Some("urn:uuid:1234".to_string()),
        publisher: Some("Editora".to_string()),
    };
    assert_eq!(meta.title.as_deref(), Some("Livro de Teste"));

    let chapter = EpubChapter {
        path: "OEBPS/ch01.xhtml".to_string(),
        content: "<p>Conteudo</p>".to_string(),
        media_type: "application/xhtml+xml".to_string(),
    };

    let book = EpubBook {
        metadata: meta,
        chapters: vec![chapter],
        resources: Vec::new(),
        raw_container_xml: Some("<container/>".to_string()),
        raw_opf_content: Some("<package/>".to_string()),
        opf_path: "OEBPS/content.opf".to_string(),
    };

    assert_eq!(book.chapters.len(), 1);
    assert_eq!(book.opf_path, "OEBPS/content.opf");
}
