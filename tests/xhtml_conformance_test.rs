use epub_translator_google::domain::services::{
    ConformanceIssue, XhtmlConformance, XhtmlValidator,
};

#[test]
fn audit_reports_duplicate_id_once_per_value() {
    let xml = r#"<body><p id="p1">a</p><p id="p1">b</p><p id="p1">c</p></body>"#;
    assert_eq!(
        XhtmlConformance::audit(xml),
        vec![ConformanceIssue::DuplicateId("p1".to_string())]
    );
}

#[test]
fn audit_reports_percentage_and_decimal_image_dimensions() {
    let xml = r#"<body><img src="a.png" width="100%" height="1.5"/></body>"#;
    assert_eq!(
        XhtmlConformance::audit(xml),
        vec![
            ConformanceIssue::NonIntegerImageDimension {
                attribute: "width".to_string(),
                value: "100%".to_string(),
            },
            ConformanceIssue::NonIntegerImageDimension {
                attribute: "height".to_string(),
                value: "1.5".to_string(),
            },
        ]
    );
}

#[test]
fn audit_reports_empty_and_negative_image_dimensions() {
    let xml = r#"<img width="" height="-1"/>"#;
    assert_eq!(XhtmlConformance::audit(xml).len(), 2);
}

#[test]
fn audit_ignores_dimensions_outside_img_and_comments() {
    let xml = r#"<body><!-- <p id="x"/><p id="x"/> --><table width="100%"><tr><td>a</td></tr></table></body>"#;
    assert!(XhtmlConformance::audit(xml).is_empty());
}

#[test]
fn audit_accepts_integer_dimensions_and_unique_ids() {
    let xml = r#"<body><p id="p1">a</p><img id="i1" width="0" height="1" src="a.png"/></body>"#;
    assert!(XhtmlConformance::audit(xml).is_empty());
}

#[test]
fn audit_empty_input_has_no_issues() {
    assert!(XhtmlConformance::audit("").is_empty());
}

#[test]
fn display_formats_issues() {
    assert_eq!(
        ConformanceIssue::DuplicateId("p1".to_string()).to_string(),
        "duplicate id 'p1'"
    );
    assert!(
        ConformanceIssue::NonIntegerImageDimension {
            attribute: "width".to_string(),
            value: "100%".to_string(),
        }
        .to_string()
        .contains("width=\"100%\"")
    );
}

#[test]
fn sanitize_removes_garbage_dimension_without_style() {
    let xml = r#"<img src="a.png" width="auto"/>"#;
    assert_eq!(XhtmlConformance::sanitize(xml), r#"<img src="a.png" />"#);
}

#[test]
fn sanitize_drops_later_duplicate_ids() {
    let xml = r#"<p id="p1">a</p><p id="p1" class="c">b</p>"#;
    assert_eq!(
        XhtmlConformance::sanitize(xml),
        r#"<p id="p1">a</p><p class="c">b</p>"#
    );
}

#[test]
fn sanitize_keeps_existing_style_property_over_attribute() {
    let xml = r#"<img src="a.png" width="100%" style="width: 50%;"/>"#;
    assert_eq!(
        XhtmlConformance::sanitize(xml),
        r#"<img src="a.png" style="width: 50%;" />"#
    );
}

#[test]
fn sanitize_merges_percentage_into_existing_style() {
    let xml = r#"<img src="a.png" style="border: 0;" width="100%"/>"#;
    assert_eq!(
        XhtmlConformance::sanitize(xml),
        r#"<img src="a.png" style="border: 0; width: 100%" />"#
    );
}

#[test]
fn sanitize_creates_style_with_px_for_decimal() {
    let xml = r#"<img src="a.png" width="12.5" height="50%"/>"#;
    assert_eq!(
        XhtmlConformance::sanitize(xml),
        r#"<img src="a.png" style="width: 12.5px; height: 50%" />"#
    );
}

#[test]
fn sanitize_leaves_conformant_markup_byte_identical() {
    let xml = "<?xml version=\"1.0\"?>\n<html><body class='x'><img src='a.png' width='10'/></body></html>";
    assert_eq!(XhtmlConformance::sanitize(xml), xml);
}

#[test]
fn sanitize_output_is_well_formed_and_audit_clean() {
    let xml = r#"<div><img alt='say "hi"' width="100%"/><p id="a">x</p><p id="a">y</p></div>"#;
    let sanitized = XhtmlConformance::sanitize(xml);
    assert!(XhtmlValidator::validate(&sanitized).is_ok());
    assert!(XhtmlConformance::audit(&sanitized).is_empty());
    assert!(sanitized.contains(r#"alt='say "hi"'"#));
}

#[test]
fn restore_reverts_changed_id_from_original() {
    let original = r#"<body><p id="p119">Hello</p><p id="p120">World</p></body>"#;
    let translated = r#"<body><p id="p120">Olá</p><p id="p120">Mundo</p></body>"#;
    assert_eq!(
        XhtmlConformance::restore_structural_attributes(original, translated),
        r#"<body><p id="p119">Olá</p><p id="p120">Mundo</p></body>"#
    );
}

#[test]
fn restore_reinserts_missing_and_removes_invented_structural_attributes() {
    let original = r#"<a href="ch1.xhtml#n1" class="link">x</a>"#;
    let translated = r#"<a title="t" id="new">y</a>"#;
    assert_eq!(
        XhtmlConformance::restore_structural_attributes(original, translated),
        r#"<a title="t" href="ch1.xhtml#n1" class="link">y</a>"#
    );
}

#[test]
fn restore_skips_tag_names_with_mismatched_counts() {
    let original = r#"<p id="a">1</p><p id="b">2</p><span id="s">3</span>"#;
    let translated = r#"<p id="z">1 2</p><span id="t">3</span>"#;
    assert_eq!(
        XhtmlConformance::restore_structural_attributes(original, translated),
        r#"<p id="z">1 2</p><span id="s">3</span>"#
    );
}

#[test]
fn restore_ignores_tags_absent_from_original() {
    let original = "<p>Hello</p>";
    let translated = r#"<translated><p>Olá</p></translated>"#;
    assert_eq!(
        XhtmlConformance::restore_structural_attributes(original, translated),
        translated
    );
}

#[test]
fn restore_preserves_translated_non_structural_attributes() {
    let original = r#"<img src="a.png" alt="Cat"/>"#;
    let translated = r#"<img src="b.png" alt="Gato"/>"#;
    assert_eq!(
        XhtmlConformance::restore_structural_attributes(original, translated),
        r#"<img src="a.png" alt="Gato" />"#
    );
}

#[test]
fn restore_returns_identical_text_when_structure_unchanged() {
    let original = r#"<p id="a" class="c">Hi</p>"#;
    let translated = r#"<p class="c" id="a">Oi</p>"#;
    assert_eq!(
        XhtmlConformance::restore_structural_attributes(original, translated),
        translated
    );
}

#[test]
fn enforce_restores_ids_and_sanitizes_images() {
    let original = r#"<div><p id="p119">a</p><img src="i.png" width="100%"/></div>"#;
    let translated = r#"<div><p id="p120">b</p><img src="i.png" width="100%"/></div>"#;
    let enforced = XhtmlConformance::enforce(original, translated);
    assert_eq!(
        enforced,
        r#"<div><p id="p119">b</p><img src="i.png" style="width: 100%" /></div>"#
    );
    assert!(XhtmlConformance::audit(&enforced).is_empty());
}

#[test]
fn heal_applies_conformance_sanitization() {
    let healed = XhtmlValidator::heal(r#"<p id="a">x</p><p id="a">y<img width="100%"></p>"#);
    assert!(XhtmlConformance::audit(&healed).is_empty());
    assert!(XhtmlValidator::validate(&healed).is_ok());
}
