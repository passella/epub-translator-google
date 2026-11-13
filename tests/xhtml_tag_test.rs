use epub_translator_google::domain::services::{
    TagAttribute, parse_attributes, render_tag, start_tags,
};

#[test]
fn start_tags_skips_comments_cdata_pi_doctype_and_end_tags() {
    let xml = "<?xml version=\"1.0\"?><!DOCTYPE html><!-- <a> --><![CDATA[<b>]]><p>x</p><br/>";
    let names: Vec<String> = start_tags(xml).map(|t| t.name).collect();
    assert_eq!(names, vec!["p".to_string(), "br".to_string()]);
}

#[test]
fn start_tags_stops_on_unterminated_tag() {
    assert_eq!(start_tags("<p>a<img src=\"x\"").count(), 1);
}

#[test]
fn start_tags_handles_unterminated_comment_and_empty_input() {
    assert_eq!(start_tags("<!-- <p>").count(), 0);
    assert_eq!(start_tags("").count(), 0);
}

#[test]
fn start_tags_reports_byte_spans() {
    let xml = "ab<p id=\"x\">c</p>";
    let tag = start_tags(xml).next().unwrap();
    assert_eq!((tag.start, tag.end), (2, 12));
    assert_eq!(tag.raw, "<p id=\"x\">");
}

#[test]
fn parse_attributes_handles_quotes_unquoted_bare_and_spacing() {
    let attrs = parse_attributes("<input a=\"1\" b='2' c=3 d e = \"5\"/>");
    let pairs: Vec<(&str, &str)> = attrs.iter().map(|a| (a.name, a.value.as_ref())).collect();
    assert_eq!(
        pairs,
        vec![("a", "1"), ("b", "2"), ("c", "3"), ("d", ""), ("e", "5")]
    );
}

#[test]
fn parse_attributes_handles_unterminated_quote_and_no_attributes() {
    let attrs = parse_attributes("<p id=\"x>");
    assert_eq!(attrs.len(), 1);
    assert!(parse_attributes("<p>").is_empty());
    assert!(parse_attributes("<br/>").is_empty());
}

#[test]
fn parse_attributes_preserves_slashes_inside_values() {
    let attrs = parse_attributes("<img src=\"a/b.png\"/>");
    assert_eq!(attrs[0].value, "a/b.png");
}

#[test]
fn render_tag_preserves_self_closing_and_switches_quote_when_needed() {
    let attrs = vec![
        TagAttribute::new("alt", "say \"hi\""),
        TagAttribute::new("src", "a.png"),
    ];
    assert_eq!(
        render_tag("<IMG/>", &attrs),
        "<IMG alt='say \"hi\"' src=\"a.png\" />"
    );
    assert_eq!(render_tag("<p class=\"x\">", &[]), "<p>");
}

#[test]
fn tag_attribute_name_match_is_case_insensitive() {
    assert!(TagAttribute::new("ID", "x").is_named("id"));
    assert!(!TagAttribute::new("idx", "x").is_named("id"));
}
