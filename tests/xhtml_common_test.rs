use epub_translator_google::domain::services::xhtml_common::{
    VOID_TAGS, extract_tag_name, find_tag_closing_bracket, is_valid_xml_entity, is_void_tag,
};

#[test]
fn test_find_tag_closing_bracket_returns_none_when_unclosed() {
    assert_eq!(find_tag_closing_bracket("<div"), None);
    assert_eq!(find_tag_closing_bracket(""), None);
    assert_eq!(find_tag_closing_bracket("hello"), None);
}

#[test]
fn test_find_tag_closing_bracket_returns_none_when_quote_unclosed_or_escaped() {
    assert_eq!(find_tag_closing_bracket(r#"<div class="hello"#), None);
    assert_eq!(find_tag_closing_bracket(r#"<div class='hello"#), None);
}

#[test]
fn test_find_tag_closing_bracket_resets_quote_on_newline_or_open_bracket() {
    assert_eq!(
        find_tag_closing_bracket("<div class=\"unclosed\n>"),
        Some(21)
    );
    assert_eq!(find_tag_closing_bracket("<div class=\"<span >"), Some(18));
}

#[test]
fn test_find_tag_closing_bracket_ignores_gt_inside_quotes() {
    assert_eq!(
        find_tag_closing_bracket(r#"<div data-formula="1 > 0" id="main">"#),
        Some(35)
    );
    assert_eq!(
        find_tag_closing_bracket(r#"<div data-formula='1 > 0' id='main'>"#),
        Some(35)
    );
}

#[test]
fn test_find_tag_closing_bracket_simple() {
    assert_eq!(find_tag_closing_bracket("<p>"), Some(2));
    assert_eq!(find_tag_closing_bracket("</p>"), Some(3));
    assert_eq!(find_tag_closing_bracket("<br />"), Some(5));
}

#[test]
fn test_extract_tag_name_empty_and_invalid_slices() {
    assert_eq!(extract_tag_name(""), "");
    assert_eq!(extract_tag_name("   "), "");
    assert_eq!(extract_tag_name("no_bracket_here"), "");
    assert_eq!(extract_tag_name("<>"), "");
    assert_eq!(extract_tag_name("</>"), "");
}

#[test]
fn test_extract_tag_name_standard_tags() {
    assert_eq!(extract_tag_name("<p>"), "p");
    assert_eq!(extract_tag_name("</p>"), "p");
    assert_eq!(extract_tag_name("<DIV class=\"box\">"), "div");
    assert_eq!(extract_tag_name("</DIV>"), "div");
    assert_eq!(extract_tag_name("  <span id=\"s1\">"), "span");
    assert_eq!(extract_tag_name("  </span >"), "span");
}

#[test]
fn test_extract_tag_name_with_namespaces_and_hyphens() {
    assert_eq!(extract_tag_name("<epub:switch>"), "epub:switch");
    assert_eq!(extract_tag_name("</epub:switch>"), "epub:switch");
    assert_eq!(
        extract_tag_name("<custom-element attr=\"1\">"),
        "custom-element"
    );
    assert_eq!(extract_tag_name("</custom-element>"), "custom-element");
    assert_eq!(extract_tag_name("<xml_tag_name>"), "xml_tag_name");
}

#[test]
fn test_is_void_tag_matches_known_void_tags() {
    for tag in VOID_TAGS {
        assert!(is_void_tag(tag));
        assert!(is_void_tag(&tag.to_uppercase()));
    }
}

#[test]
fn test_is_void_tag_rejects_non_void_tags() {
    assert!(!is_void_tag("p"));
    assert!(!is_void_tag("div"));
    assert!(!is_void_tag("span"));
    assert!(!is_void_tag("html"));
    assert!(!is_void_tag("body"));
    assert!(!is_void_tag(""));
}

#[test]
fn test_is_valid_xml_entity_rejects_missing_ampersand_or_semicolon() {
    assert!(!is_valid_xml_entity(""));
    assert!(!is_valid_xml_entity("amp;"));
    assert!(!is_valid_xml_entity("&"));
    assert!(!is_valid_xml_entity("&amp"));
}

#[test]
fn test_is_valid_xml_entity_rejects_invalid_lengths() {
    assert!(!is_valid_xml_entity("&;"));
    assert!(!is_valid_xml_entity("&abcdefghijk;"));
}

#[test]
fn test_is_valid_xml_entity_named_entities() {
    assert!(is_valid_xml_entity("&amp;"));
    assert!(is_valid_xml_entity("&lt;"));
    assert!(is_valid_xml_entity("&gt;"));
    assert!(is_valid_xml_entity("&quot;"));
    assert!(is_valid_xml_entity("&apos;"));
    assert!(is_valid_xml_entity("&nbsp;"));
    assert!(is_valid_xml_entity("&mdash;"));
    assert!(is_valid_xml_entity("&ent_1;"));
    assert!(!is_valid_xml_entity("&inv@lid;"));
}

#[test]
fn test_is_valid_xml_entity_decimal_and_hex() {
    assert!(is_valid_xml_entity("&#38;"));
    assert!(is_valid_xml_entity("&#x26;"));
    assert!(is_valid_xml_entity("&#X26;"));
    assert!(is_valid_xml_entity("&#x2F;"));
    assert!(!is_valid_xml_entity("&#;"));
    assert!(!is_valid_xml_entity("&#x;"));
    assert!(!is_valid_xml_entity("&#X;"));
    assert!(!is_valid_xml_entity("&#38a;"));
    assert!(!is_valid_xml_entity("&#x2G;"));
}
