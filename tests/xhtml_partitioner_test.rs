use epub_translator_google::domain::services::{PlanSegment, XhtmlDocumentPlan, XhtmlPartitioner};

#[test]
fn test_xhtml_partitioner_threshold_configuration_and_defaults() {
    let default_partitioner = XhtmlPartitioner::default();
    assert_eq!(
        default_partitioner.threshold(),
        XhtmlPartitioner::DEFAULT_THRESHOLD
    );

    let custom_partitioner = XhtmlPartitioner::with_threshold(500);
    assert_eq!(custom_partitioner.threshold(), 500);

    let zero_threshold = XhtmlPartitioner::with_threshold(0);
    assert_eq!(zero_threshold.threshold(), 1);
}

#[test]
fn test_xhtml_partitioner_is_giant_boundary_values() {
    let threshold = 50;
    let partitioner = XhtmlPartitioner::with_threshold(threshold);

    let text_49 = "a".repeat(threshold - 1);
    let text_50 = "a".repeat(threshold);
    let text_51 = "a".repeat(threshold + 1);

    assert!(!partitioner.is_giant(&text_49));
    assert!(!partitioner.is_giant(&text_50));
    assert!(partitioner.is_giant(&text_51));
    assert!(!partitioner.is_giant(""));
}

#[test]
fn test_xhtml_partitioner_empty_and_whitespace_content() {
    let partitioner = XhtmlPartitioner::with_threshold(10);
    let empty_plan = partitioner.partition("");
    assert_eq!(empty_plan.prelude, "");
    assert_eq!(empty_plan.postlude, "");
    assert!(empty_plan.segments.is_empty());
    assert_eq!(empty_plan.recompose::<&str>(&[]), "");

    let ws_content = "   \n\t  ";
    let ws_plan = partitioner.partition(ws_content);
    assert_eq!(
        ws_plan.recompose(&ws_plan.translatable_chunks()),
        ws_content
    );
}

#[test]
fn test_xhtml_partitioner_document_without_body_tag() {
    let html = r#"<div class="fragment"><p>First</p><p>Second</p></div>"#;
    let partitioner = XhtmlPartitioner::with_threshold(20);
    let plan = partitioner.partition(html);

    assert_eq!(plan.prelude, "");
    assert_eq!(plan.postlude, "");
    assert_eq!(plan.recompose(&plan.translatable_chunks()), html);

    let chunks = plan.translatable_chunks();
    assert!(chunks.len() >= 2);
    for chunk in chunks {
        assert!(chunk.starts_with("<p>") || chunk.starts_with("<div") || chunk.ends_with("</p>"));
    }
}

#[test]
fn test_xhtml_partitioner_with_comments_cdata_and_declarations() {
    let html = r#"<?xml version="1.0"?>
<!DOCTYPE html>
<html>
<head><title>Meta</title></head>
<body>
<!-- This is a comment -->
<![CDATA[Unescaped <data> inside]]>
<p>Visible content</p>
</body>
</html>"#;

    let partitioner = XhtmlPartitioner::with_threshold(30);
    let plan = partitioner.partition(html);

    assert!(plan.prelude.contains("<body>"));
    assert!(plan.postlude.contains("</body>"));
    assert_eq!(plan.recompose(&plan.translatable_chunks()), html);
}

#[test]
fn test_xhtml_partitioner_self_closing_and_void_tags() {
    let html =
        r#"<html><body><p>Text<br/>Next</p><hr/><img src="cover.jpg" alt="Cover"/></body></html>"#;
    let partitioner = XhtmlPartitioner::with_threshold(20);
    let plan = partitioner.partition(html);

    assert_eq!(plan.recompose(&plan.translatable_chunks()), html);
}

#[test]
fn test_xhtml_partitioner_attributes_with_quotes_and_brackets() {
    let html = r#"<html><body><p><a href="http://example.com?x=1&gt;2" title='Quoted "Title"'>Link</a></p></body></html>"#;
    let partitioner = XhtmlPartitioner::with_threshold(30);
    let plan = partitioner.partition(html);

    assert_eq!(plan.recompose(&plan.translatable_chunks()), html);
}

#[test]
fn test_xhtml_partitioner_deeply_nested_containers() {
    let html = r#"<html>
<body>
  <div class="level-1">
    <section class="level-2">
      <article class="level-3">
        <p>Paragraph A</p>
        <p>Paragraph B</p>
        <p>Paragraph C</p>
      </article>
    </section>
  </div>
</body>
</html>"#;

    let partitioner = XhtmlPartitioner::with_threshold(60);
    let plan = partitioner.partition(html);

    assert_eq!(plan.recompose(&plan.translatable_chunks()), html);

    let chunks = plan.translatable_chunks();
    assert!(chunks.len() >= 2);
}

#[test]
fn test_xhtml_partitioner_recompose_with_translated_chunks() {
    let html = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>Original Title</title></head>
<body>
  <h1>Chapter 1</h1>
  <p>The quick brown fox.</p>
  <p>Jumps over the lazy dog.</p>
</body>
</html>"#;

    let partitioner = XhtmlPartitioner::with_threshold(50);
    let plan = partitioner.partition(html);

    let chunks = plan.translatable_chunks();
    assert!(chunks.len() >= 2);

    let translated: Vec<String> = chunks
        .iter()
        .map(|c| {
            c.replace("Chapter 1", "Capítulo 1")
                .replace("The quick brown fox.", "A rápida raposa marrom.")
                .replace("Jumps over the lazy dog.", "Salta sobre o cão preguiçoso.")
        })
        .collect();

    let recomposed = plan.recompose(&translated);

    assert!(recomposed.starts_with(r#"<?xml version="1.0" encoding="utf-8"?>"#));
    assert!(recomposed.contains("<title>Original Title</title>"));
    assert!(recomposed.contains("<h1>Capítulo 1</h1>"));
    assert!(recomposed.contains("<p>A rápida raposa marrom.</p>"));
    assert!(recomposed.contains("<p>Salta sobre o cão preguiçoso.</p>"));
    assert!(recomposed.ends_with("</body>\n</html>"));
}

#[test]
fn test_xhtml_document_plan_manual_construction_and_accessors() {
    let plan = XhtmlDocumentPlan {
        prelude: "PREFIX:".to_string(),
        segments: vec![
            PlanSegment::Verbatim("<wrapper>".to_string()),
            PlanSegment::Translatable("<p>Item 1</p>".to_string()),
            PlanSegment::Verbatim("<sep/>".to_string()),
            PlanSegment::Translatable("<p>Item 2</p>".to_string()),
            PlanSegment::Verbatim("</wrapper>".to_string()),
        ],
        postlude: ":SUFFIX".to_string(),
    };

    assert_eq!(
        plan.translatable_chunks(),
        vec!["<p>Item 1</p>", "<p>Item 2</p>"]
    );

    let translated = vec!["<p>Item Um</p>".to_string(), "<p>Item Dois</p>".to_string()];
    let result = plan.recompose(&translated);
    assert_eq!(
        result,
        "PREFIX:<wrapper><p>Item Um</p><sep/><p>Item Dois</p></wrapper>:SUFFIX"
    );
}
