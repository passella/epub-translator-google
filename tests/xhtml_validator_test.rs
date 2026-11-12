use epub_translator_google::domain::services::{XhtmlValidationError, XhtmlValidator};

#[test]
fn test_validate_rejects_empty_and_whitespace_inputs() {
    assert_eq!(
        XhtmlValidator::validate(""),
        Err(XhtmlValidationError::EmptyOutput)
    );
    assert_eq!(
        XhtmlValidator::validate("   \n\t  "),
        Err(XhtmlValidationError::EmptyOutput)
    );
    assert_eq!(
        XhtmlValidator::heal_and_validate(""),
        Err(XhtmlValidationError::EmptyOutput)
    );
    assert_eq!(
        XhtmlValidator::heal_and_validate("   \r\n  "),
        Err(XhtmlValidationError::EmptyOutput)
    );
}

#[test]
fn test_validate_accepts_valid_well_formed_xml() {
    let valid_doc = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml">
  <head>
    <title>Teste</title>
  </head>
  <body>
    <p class="intro">Texto de exemplo</p>
    <img src="cover.jpg" alt="Capa" />
  </body>
</html>"#;

    assert!(XhtmlValidator::validate(valid_doc).is_ok());
    let healed = XhtmlValidator::heal_and_validate(valid_doc).unwrap();
    assert_eq!(healed, valid_doc);
}

#[test]
fn test_heal_unclosed_attribute_quotes_single() {
    let malformed = r#"<p class="note-para>Este é um texto com aspas abertas.</p>"#;
    let healed = XhtmlValidator::heal_and_validate(malformed).unwrap();
    assert!(XhtmlValidator::validate(&healed).is_ok());
    assert_eq!(
        healed,
        r#"<p class="note-para">Este é um texto com aspas abertas.</p>"#
    );
}

#[test]
fn test_heal_unclosed_attribute_quotes_multiple_attributes() {
    let malformed = r#"<div class="container id="main-content">Conteúdo</div>"#;
    let healed = XhtmlValidator::heal_and_validate(malformed).unwrap();
    assert!(XhtmlValidator::validate(&healed).is_ok());
    assert_eq!(
        healed,
        r#"<div class="container" id="main-content">Conteúdo</div>"#
    );
}

#[test]
fn test_heal_unclosed_attribute_quotes_single_quotes() {
    let malformed = r#"<p class='note-para>Texto com aspas simples</p>"#;
    let healed = XhtmlValidator::heal_and_validate(malformed).unwrap();
    assert!(XhtmlValidator::validate(&healed).is_ok());
    assert_eq!(
        healed,
        r#"<p class='note-para'>Texto com aspas simples</p>"#
    );
}

#[test]
fn test_heal_mismatched_and_unclosed_nested_tags() {
    let malformed = r#"<div><p>Primeiro parágrafo sem fechar</div>"#;
    let healed = XhtmlValidator::heal_and_validate(malformed).unwrap();
    assert!(XhtmlValidator::validate(&healed).is_ok());
    assert_eq!(healed, r#"<div><p>Primeiro parágrafo sem fechar</p></div>"#);
}

#[test]
fn test_heal_deeply_nested_unclosed_tags_before_parent_close() {
    let malformed = r#"<section><div><p><span>Texto profundo</section>"#;
    let healed = XhtmlValidator::heal_and_validate(malformed).unwrap();
    assert!(XhtmlValidator::validate(&healed).is_ok());
    assert_eq!(
        healed,
        r#"<section><div><p><span>Texto profundo</span></p></div></section>"#
    );
}

#[test]
fn test_heal_truncated_content_at_eof() {
    let malformed = r#"<html><body><div><p>Texto truncado sem encerramento"#;
    let healed = XhtmlValidator::heal_and_validate(malformed).unwrap();
    assert!(XhtmlValidator::validate(&healed).is_ok());
    assert_eq!(
        healed,
        r#"<html><body><div><p>Texto truncado sem encerramento</p></div></body></html>"#
    );
}

#[test]
fn test_heal_html_void_tags_to_xml_self_closing() {
    let malformed = r#"<p>Linha 1<br>Linha 2<hr><img src="pic.png"></p>"#;
    let healed = XhtmlValidator::heal_and_validate(malformed).unwrap();
    assert!(XhtmlValidator::validate(&healed).is_ok());
    assert_eq!(
        healed,
        r#"<p>Linha 1<br />Linha 2<hr /><img src="pic.png" /></p>"#
    );
}

#[test]
fn test_heal_stray_orphan_closing_tags_are_discarded() {
    let malformed = r#"</span><p>Texto válido</p></div>"#;
    let healed = XhtmlValidator::heal_and_validate(malformed).unwrap();
    assert!(XhtmlValidator::validate(&healed).is_ok());
    assert_eq!(healed, r#"<p>Texto válido</p>"#);
}

#[test]
fn test_heal_naked_ampersands_in_text_content() {
    let malformed = r#"<p>AT&T e Q&A e A & B com entidade &amp; existente</p>"#;
    let healed = XhtmlValidator::heal_and_validate(malformed).unwrap();
    assert!(XhtmlValidator::validate(&healed).is_ok());
    assert_eq!(
        healed,
        r#"<p>AT&amp;T e Q&amp;A e A &amp; B com entidade &amp; existente</p>"#
    );
}

#[test]
fn test_heal_interleaved_section_div_tags() {
    let malformed = r#"<section><div><p class="note-para>Texto</p></section></div>"#;
    let healed = XhtmlValidator::heal_and_validate(malformed).unwrap();
    assert!(XhtmlValidator::validate(&healed).is_ok());
    assert_eq!(
        healed,
        r#"<section><div><p class="note-para">Texto</p></div></section>"#
    );
}

#[test]
fn test_heal_combined_real_world_case() {

    let malformed =
        r#"<div class="note-box><p>Aviso: AT&T informou que <br>o serviço falhou</div>"#;
    let healed = XhtmlValidator::heal_and_validate(malformed).unwrap();
    assert!(XhtmlValidator::validate(&healed).is_ok());
    assert_eq!(
        healed,
        r#"<div class="note-box"><p>Aviso: AT&amp;T informou que <br />o serviço falhou</p></div>"#
    );
}
