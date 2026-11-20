use async_trait::async_trait;
use epub_translator_google::adapters::cache::SqliteCacheAdapter;
use epub_translator_google::domain::entities::{
    EpubBook, EpubChapter, EpubMetadata, Language, TranslationResult,
};
use epub_translator_google::domain::errors::{AppError, EpubError, TranslationError};
use epub_translator_google::domain::ports::{CachePort, EpubPort, TranslatorPort};
use epub_translator_google::domain::services::{
    TagAttribute, TranslateBookUseCase, XhtmlConformance, XhtmlPartitioner, XhtmlValidator,
    render_tag,
};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

#[test]
fn appsec02_partitioner_handles_deep_nesting_exceeding_500_tags_and_100_kib_without_stack_overflow()
{
    let depth = 600usize;
    let payload_unit = "<p>Conteúdo literário suficientemente extenso para testar particionamento em profundidade extrema.</p>";
    let repeated_payload = payload_unit.repeat(1200);

    let mut html = String::with_capacity(repeated_payload.len() + depth * 32 + 64);
    html.push_str("<html><body>");
    for i in 0..depth {
        html.push_str(&format!("<div class=\"layer-{i}\">"));
    }
    html.push_str(&repeated_payload);
    for _ in 0..depth {
        html.push_str("</div>");
    }
    html.push_str("</body></html>");

    assert!(html.len() > 100 * 1024);

    let default_partitioner = XhtmlPartitioner::new();
    let default_plan = default_partitioner.partition(&html);
    let default_chunks = default_plan.translatable_chunks();
    assert!(!default_chunks.is_empty());
    assert_eq!(default_plan.recompose(&default_chunks), html);

    let small_threshold_partitioner = XhtmlPartitioner::with_threshold(512);
    let small_plan = small_threshold_partitioner.partition(&html);
    let small_chunks = small_plan.translatable_chunks();
    assert!(!small_chunks.is_empty());
    assert_eq!(small_plan.recompose(&small_chunks), html);
}

#[test]
fn appsec02_partitioner_bva_around_max_nesting_depth_256() {
    assert_eq!(XhtmlPartitioner::MAX_NESTING_DEPTH, 256);

    let build_nested = |levels: usize| -> String {
        let mut doc = String::from("<body>");
        for i in 0..levels {
            doc.push_str(&format!("<div data-lvl=\"{i}\">"));
        }
        doc.push_str("<p>Alpha paragraph</p><p>Beta paragraph</p>");
        for _ in 0..levels {
            doc.push_str("</div>");
        }
        doc.push_str("</body>");
        doc
    };

    let partitioner = XhtmlPartitioner::with_threshold(20);

    for boundary in [
        XhtmlPartitioner::MAX_NESTING_DEPTH - 1,
        XhtmlPartitioner::MAX_NESTING_DEPTH,
        XhtmlPartitioner::MAX_NESTING_DEPTH + 1,
        550,
    ] {
        let html = build_nested(boundary);
        let plan = partitioner.partition(&html);
        let chunks = plan.translatable_chunks();
        assert!(!chunks.is_empty());
        assert_eq!(plan.recompose(&chunks), html);
    }

    let capped_html = build_nested(XhtmlPartitioner::MAX_NESTING_DEPTH + 50);
    let capped_plan = partitioner.partition(&capped_html);
    let max_leaf = capped_plan
        .translatable_chunks()
        .into_iter()
        .max_by_key(|c| c.len())
        .unwrap();
    assert!(max_leaf.contains("<div data-lvl=\"256\">"));
}

#[test]
fn appsec02_partitioner_handles_unclosed_deep_tags_over_100_kib() {
    let depth = 600usize;
    let pad = "a".repeat(200);
    let mut html = String::with_capacity(140 * 1024);
    html.push_str("<html><body>");
    for i in 0..depth {
        html.push_str(&format!("<div data-pad=\"{pad}-{i}\">"));
    }
    html.push_str("<p>Tail content</p>");
    html.push_str("</body></html>");

    assert!(html.len() > 100 * 1024);

    let partitioner = XhtmlPartitioner::new();
    let plan = partitioner.partition(&html);
    let chunks = plan.translatable_chunks();
    assert!(!chunks.is_empty());
    assert_eq!(plan.recompose(&chunks), html);
}

#[test]
fn appsec03_render_tag_escapes_mixed_quotes_and_xml_metacharacters() {
    let payload_attrs = vec![TagAttribute::new(
        "title",
        r#"evil" 'attr' <script>alert(1)</script> & raw"#,
    )];
    let rendered = render_tag("<p/>", &payload_attrs);
    assert!(rendered.starts_with("<p title=\""));
    assert!(!rendered.contains("<script>"));
    assert!(rendered.contains("&quot;"));
    assert!(rendered.contains("&lt;script&gt;"));
    assert!(rendered.contains("&amp; raw"));
    assert!(XhtmlValidator::validate(&rendered).is_ok());

    let mixed_quotes_attrs = vec![TagAttribute::new("alt", r#"a"b'c"#)];
    let mixed_rendered = render_tag("<img/>", &mixed_quotes_attrs);
    assert_eq!(mixed_rendered, "<img alt=\"a&quot;b'c\" />");
    assert!(XhtmlValidator::validate(&mixed_rendered).is_ok());
}

#[test]
fn appsec03_render_tag_preserves_existing_entities_without_double_escaping() {
    let attrs = vec![
        TagAttribute::new("href", "https://example.com?a=1&amp;b=2&c=3"),
        TagAttribute::new(
            "title",
            "Quoted &quot;Word&quot; &apos;Single&apos; &#39; &#x22;",
        ),
        TagAttribute::new("data-expr", "x < y && y > z"),
    ];
    let rendered = render_tag("<a>", &attrs);
    let doc = format!("{rendered}Link</a>");
    assert!(rendered.contains("a=1&amp;b=2&amp;c=3"));
    assert!(!rendered.contains("&amp;amp;"));
    assert!(rendered.contains("&quot;Word&quot;"));
    assert!(!rendered.contains("&amp;quot;"));
    assert!(rendered.contains("&lt; y &amp;&amp; y &gt; z"));
    assert!(XhtmlValidator::validate(&doc).is_ok());
}

#[test]
fn appsec03_render_tag_keeps_single_quote_optimization_when_only_double_quotes_present() {
    let attrs = vec![TagAttribute::new("alt", r#"say "hi""#)];
    assert_eq!(render_tag("<img/>", &attrs), r#"<img alt='say "hi"' />"#);
}

#[test]
fn appsec03_css_declaration_blocks_css_injection_and_invalid_dimensions() {
    let injection_cases = [
        r#"<img src="a.png" width="10px; background: url(https://evil.test/track.png)"/>"#,
        r#"<img src="a.png" width="10px; background: url(javascript:alert(1))"/>"#,
        r#"<img src="a.png" width="10px: expression(alert(1))"/>"#,
        r#"<img src="a.png" height="50%; color: red"/>"#,
        r#"<img src="a.png" width="10px\" onclick=\"alert(1)"/>"#,
        r#"<img src="a.png" width=".5px"/>"#,
        r#"<img src="a.png" width="12.px"/>"#,
        r#"<img src="a.png" width="12."/>"#,
        r#"<img src="a.png" width="-10px"/>"#,
        r#"<img src="a.png" width="10 px"/>"#,
        r#"<img src="a.png" width="10vmin"/>"#,
    ];

    for malicious in injection_cases {
        let sanitized = XhtmlConformance::sanitize(malicious);
        assert_eq!(
            sanitized, r#"<img src="a.png" />"#,
            "failed on: {malicious}"
        );
        assert!(XhtmlValidator::validate(&sanitized).is_ok());
        assert!(XhtmlConformance::audit(&sanitized).is_empty());
    }
}

#[test]
fn appsec03_css_declaration_allows_all_strict_valid_units() {
    let units = ["px", "em", "rem", "%", "pt", "cm", "mm", "in", "vw", "vh"];
    for unit in units {
        let xml = format!(r#"<img src="a.png" width="12.5{unit}"/>"#);
        let sanitized = XhtmlConformance::sanitize(&xml);
        assert_eq!(
            sanitized,
            format!(r#"<img src="a.png" style="width: 12.5{unit}" />"#)
        );
        assert!(XhtmlValidator::validate(&sanitized).is_ok());
    }

    let unitless_decimal = r#"<img src="a.png" width="12.5"/>"#;
    assert_eq!(
        XhtmlConformance::sanitize(unitless_decimal),
        r#"<img src="a.png" style="width: 12.5px" />"#
    );
}

#[test]
fn appsec04_sanitize_removes_active_tags_across_variants() {
    let cases = [
        (
            r#"<div><p>Antes</p><script>alert('xss')</script><p>Depois</p></div>"#,
            r#"<div><p>Antes</p><p>Depois</p></div>"#,
        ),
        (
            r#"<div><SCRIPT type="text/javascript">if (1 < 2) { alert(1); }</SCRIPT><p>OK</p></div>"#,
            r#"<div><p>OK</p></div>"#,
        ),
        (
            r#"<div><script src="https://evil.test/x.js" /><p>OK</p></div>"#,
            r#"<div><p>OK</p></div>"#,
        ),
        (
            r#"<div><svg:script>alert(1)</svg:script><p>OK</p></div>"#,
            r#"<div><p>OK</p></div>"#,
        ),
        (
            r#"<div><iframe src="https://evil.test"><span>fallback</span></iframe><p>OK</p></div>"#,
            r#"<div><p>OK</p></div>"#,
        ),
        (
            r#"<div><iframe src="https://evil.test" /><p>OK</p></div>"#,
            r#"<div><p>OK</p></div>"#,
        ),
        (
            r#"<div><object data="evil.swf"><param name="a" value="b"/><object data="inner">x</object></object><p>OK</p></div>"#,
            r#"<div><p>OK</p></div>"#,
        ),
        (
            r#"<div><embed src="evil.swf" /><p>OK</p></div>"#,
            r#"<div><p>OK</p></div>"#,
        ),
        (
            r#"<div><embed src="evil.swf"><p>OK</p></div>"#,
            r#"<div><p>OK</p></div>"#,
        ),
        (
            r#"<div><embed src="evil.swf"></embed><p>OK</p></div>"#,
            r#"<div><p>OK</p></div>"#,
        ),
    ];

    for (input, expected) in cases {
        let sanitized = XhtmlConformance::sanitize(input);
        assert_eq!(sanitized, expected, "failed on: {input}");
        assert!(XhtmlValidator::validate(&sanitized).is_ok());
    }
}

#[test]
fn appsec04_sanitize_strips_event_handlers_and_dangerous_uri_schemes() {
    let cases = [
        (
            r#"<p onclick="alert(1)" ONERROR="steal()" onMouseOver="x()" class="ok">Texto</p>"#,
            r#"<p class="ok">Texto</p>"#,
        ),
        (
            r#"<a href="javascript:alert(1)">Link</a>"#,
            r#"<a>Link</a>"#,
        ),
        (
            r#"<a href="  JaVa&#x09;ScRiPt&#58;alert(1)">Link</a>"#,
            r#"<a>Link</a>"#,
        ),
        (
            r#"<a href="&#0000106;avascript&colon;alert(1)">Link</a>"#,
            r#"<a>Link</a>"#,
        ),
        (
            r#"<img src="vbscript:msgbox(1)" alt="capa"/>"#,
            r#"<img alt="capa" />"#,
        ),
        (
            r#"<form action="data: text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg=="><p>Form</p></form>"#,
            r#"<form><p>Form</p></form>"#,
        ),
        (
            r#"<a xlink:href="data:text&#47;html,&lt;script&gt;alert(1)&lt;/script&gt;">SVG</a>"#,
            r#"<a>SVG</a>"#,
        ),
        (
            r#"<a svg:href="&#xA0;javascript:alert(1)">NS</a>"#,
            r#"<a>NS</a>"#,
        ),
        (
            r#"<p/onclick="alert(1)">SlashAttr</p>"#,
            r#"<p>SlashAttr</p>"#,
        ),
        (
            r#"<img src="a.png"/onerror="alert(1)"/>"#,
            r#"<img src="a.png" />"#,
        ),
        (
            r#"<p style="background: url(java/*c*/script:alert(1)); color: red">Texto</p>"#,
            r#"<p>Texto</p>"#,
        ),
    ];

    for (input, expected) in cases {
        let sanitized = XhtmlConformance::sanitize(input);
        assert_eq!(sanitized, expected, "failed on: {input}");
        assert!(XhtmlValidator::validate(&sanitized).is_ok());
    }
}

#[test]
fn appsec04_sanitize_preserves_safe_uris_comments_and_escaped_text() {
    let safe = concat!(
        "<!-- <script>alert(1)</script> -->",
        r#"<p class="note"><a href="chapter2.xhtml#sec1">Next</a>"#,
        r#"<a href="https://example.com/?next=javascript:void(0)">Ext</a>"#,
        r#"<img src="data:image/png;base64,iVBORw0KGgo=" width="100" height="50" />"#,
        "&lt;script&gt;literal&lt;/script&gt;</p>"
    );
    let sanitized = XhtmlConformance::sanitize(safe);
    assert_eq!(sanitized, safe);
    assert!(XhtmlValidator::validate(&sanitized).is_ok());
}

#[test]
fn appsec04_validator_heal_and_validate_neutralizes_well_formed_active_payloads() {
    let well_formed_xss = r#"<div><p onclick="alert(1)">Olá</p><script>alert(2)</script><a href="javascript:alert(3)">Clique</a></div>"#;
    let cleaned = XhtmlValidator::heal_and_validate(well_formed_xss).unwrap();
    assert_eq!(cleaned, r#"<div><p>Olá</p><a>Clique</a></div>"#);
    assert!(XhtmlValidator::validate(&cleaned).is_ok());
}

struct XssInjectingTranslator {
    payload: String,
}

#[async_trait]
impl TranslatorPort for XssInjectingTranslator {
    async fn translate_html(
        &self,
        _html: &str,
        _source_lang: &Language,
        _target_lang: &Language,
    ) -> Result<TranslationResult, TranslationError> {
        Ok(TranslationResult {
            translated_text: self.payload.clone(),
            detected_source: Some(Language::new("en")),
        })
    }

    async fn translate_batch(
        &self,
        texts: &[String],
        source_lang: &Language,
        target_lang: &Language,
    ) -> Result<Vec<TranslationResult>, TranslationError> {
        let mut results = Vec::new();
        for text in texts {
            results.push(self.translate_html(text, source_lang, target_lang).await?);
        }
        Ok(results)
    }

    fn provider_name(&self) -> &'static str {
        "xss-injecting-translator"
    }
}

struct SecurityMockEpubPort {
    book: Mutex<EpubBook>,
    written_book: Mutex<Option<EpubBook>>,
}

#[async_trait]
impl EpubPort for SecurityMockEpubPort {
    async fn read_epub(&self, _path: &Path) -> Result<EpubBook, EpubError> {
        Ok(self.book.lock().await.clone())
    }

    async fn write_epub(&self, book: &EpubBook, _path: &Path) -> Result<(), EpubError> {
        *self.written_book.lock().await = Some(book.clone());
        Ok(())
    }

    async fn verify_epub(
        &self,
        _path: &Path,
    ) -> Result<epub_translator_google::domain::entities::EpubVerificationReport, EpubError> {
        Ok(
            epub_translator_google::domain::entities::EpubVerificationReport {
                is_valid: true,
                mimetype_valid: true,
                container_xml_valid: true,
                opf_valid: true,
                total_xhtml_files: 1,
                valid_xhtml_files: 1,
                errors: Vec::new(),
            },
        )
    }
}

fn build_single_chapter_book(content: &str) -> EpubBook {
    EpubBook {
        metadata: EpubMetadata {
            title: Some("Security Book".to_string()),
            language: Some(Language::new("en")),
            creator: None,
            identifier: None,
            publisher: None,
        },
        chapters: vec![EpubChapter {
            path: "OEBPS/ch1.xhtml".to_string(),
            content: content.to_string(),
            media_type: "application/xhtml+xml".to_string(),
        }],
        resources: Vec::new(),
        raw_container_xml: Some("<container/>".to_string()),
        raw_opf_content: Some("<package><dc:language>en</dc:language></package>".to_string()),
        opf_path: "OEBPS/content.opf".to_string(),
    }
}

#[tokio::test]
async fn appsec03_appsec04_translate_book_sanitizes_and_validates_post_conformance() {
    let db_path = std::env::temp_dir().join("test_xhtml_security_pipeline.db");
    let _ = fs::remove_file(&db_path);
    let cache_adapter = Arc::new(SqliteCacheAdapter::new(&db_path).unwrap());

    let original = r#"<body><p id="p1" title="a &quot;b&quot; 'c'">Hello</p><img src="cover.png" width="100%" alt="Cover"/></body>"#;
    let translated_with_xss = concat!(
        r#"<body><p id="p99" onclick="alert(1)" title="a &quot;b&quot; 'c'">Olá</p>"#,
        r#"<script>alert('xss')</script>"#,
        r#"<iframe src="https://evil.test">x</iframe>"#,
        r#"<a href="javascript:alert(2)">Link</a>"#,
        r#"<img src="evil.png" width="10px; background: url(javascript:alert(3))" alt="Capa"/></body>"#
    );

    let epub_port = Arc::new(SecurityMockEpubPort {
        book: Mutex::new(build_single_chapter_book(original)),
        written_book: Mutex::new(None),
    });

    let use_case = TranslateBookUseCase::new(
        epub_port.clone(),
        Arc::new(XssInjectingTranslator {
            payload: translated_with_xss.to_string(),
        }),
        cache_adapter.clone(),
        None,
    );

    use_case
        .execute(
            Path::new("in.epub"),
            Path::new("out.epub"),
            &Language::new("en"),
            &Language::new("pt-BR"),
        )
        .await
        .unwrap();

    let written = epub_port.written_book.lock().await;
    let final_content = &written.as_ref().unwrap().chapters[0].content;

    assert!(!final_content.contains("<script"));
    assert!(!final_content.contains("<iframe"));
    assert!(!final_content.contains("onclick"));
    assert!(!final_content.contains("javascript:"));
    assert!(!final_content.contains("background:"));
    assert!(final_content.contains(r#"id="p1""#));
    assert!(final_content.contains(r#"src="cover.png""#));
    assert!(XhtmlValidator::validate(final_content).is_ok());
    assert!(XhtmlConformance::audit(final_content).is_empty());

    let _ = fs::remove_file(&db_path);
}

#[tokio::test]
async fn appsec03_appsec04_translate_book_rejects_output_reduced_to_empty_after_sanitization() {
    let db_path = std::env::temp_dir().join("test_xhtml_security_empty_after_strip.db");
    let _ = fs::remove_file(&db_path);
    let cache_adapter = Arc::new(SqliteCacheAdapter::new(&db_path).unwrap());

    let epub_port = Arc::new(SecurityMockEpubPort {
        book: Mutex::new(build_single_chapter_book("<p>Hello</p>")),
        written_book: Mutex::new(None),
    });

    let use_case = TranslateBookUseCase::new(
        epub_port,
        Arc::new(XssInjectingTranslator {
            payload: "<script>alert(1)</script>".to_string(),
        }),
        cache_adapter,
        None,
    );

    let err = use_case
        .execute(
            Path::new("in.epub"),
            Path::new("out.epub"),
            &Language::new("en"),
            &Language::new("pt-BR"),
        )
        .await
        .unwrap_err();

    match err {
        AppError::ChapterTranslation(boxed) => {
            assert!(matches!(boxed.source, TranslationError::XmlValidation(_)));
        }
        other => panic!("Expected ChapterTranslation XmlValidation error, got {other:?}"),
    }

    let _ = fs::remove_file(&db_path);
}

#[tokio::test]
async fn appsec04_translate_book_sanitizes_poisoned_cache_entry_on_hit() {
    let db_path = std::env::temp_dir().join("test_xhtml_security_poisoned_cache.db");
    let _ = fs::remove_file(&db_path);
    let cache_adapter = Arc::new(SqliteCacheAdapter::new(&db_path).unwrap());

    let original = r#"<p id="p1">Hello</p>"#;
    let poisoned = r#"<p id="p9" onclick="alert(1)">Olá</p><script>steal()</script>"#;
    cache_adapter
        .set(
            "en",
            "pt-BR",
            "xss-injecting-translator",
            original,
            poisoned,
        )
        .await
        .unwrap();

    let epub_port = Arc::new(SecurityMockEpubPort {
        book: Mutex::new(build_single_chapter_book(original)),
        written_book: Mutex::new(None),
    });

    let use_case = TranslateBookUseCase::new(
        epub_port.clone(),
        Arc::new(XssInjectingTranslator {
            payload: "<p>unused</p>".to_string(),
        }),
        cache_adapter,
        None,
    );

    let metrics = use_case
        .execute(
            Path::new("in.epub"),
            Path::new("out.epub"),
            &Language::new("en"),
            &Language::new("pt-BR"),
        )
        .await
        .unwrap();

    assert_eq!(metrics.cache_hits, 1);
    let written = epub_port.written_book.lock().await;
    let final_content = &written.as_ref().unwrap().chapters[0].content;
    assert_eq!(final_content, r#"<p id="p1">Olá</p>"#);
    assert!(XhtmlValidator::validate(final_content).is_ok());

    let _ = fs::remove_file(&db_path);
}

#[test]
fn bva_partitioner_multichunk_with_multibyte_utf8_emojis_cjk_and_combining_marks() {
    let html = concat!(
        "<body><div class=\"wrap\">",
        "<p>日本語テストの段落です。漢字とひらがな。</p>",
        "<p>Emojis 🚀🔥🧪🧑‍💻 e acentos combinados e\u{0301} a\u{0308} o\u{0303}!</p>",
        "<p>Mais texto multibyte: Café, naïve, résumé, 世界こんにちは 🎉</p>",
        "</div></body>"
    );

    let partitioner = XhtmlPartitioner::with_threshold(40);
    let plan = partitioner.partition(html);
    let chunks = plan.translatable_chunks();
    assert!(chunks.len() >= 3);
    for chunk in &chunks {
        assert!(std::str::from_utf8(chunk.as_bytes()).is_ok());
    }
    assert_eq!(plan.recompose(&chunks), html);
}

#[test]
fn bva_conformance_handles_empty_and_bare_attributes() {
    let input = r#"<div><img src="pic.png" id="" width="" /><input disabled class="chk" /></div>"#;
    let sanitized = XhtmlConformance::sanitize(input);
    assert!(!sanitized.contains("width=\"\""));
    assert!(sanitized.contains("src=\"pic.png\""));
    assert!(sanitized.contains("disabled"));

    let issues = XhtmlConformance::audit(
        r#"<div><img src="a.png" width="" /><p id="">A</p><p id="">B</p></div>"#,
    );
    assert!(issues.len() >= 2);
}

#[test]
fn appsec04_sanitize_strips_heterogeneous_nested_active_tags() {
    let nested = concat!(
        "<div>",
        "<object data=\"a.swf\"><iframe src=\"https://evil.test\"><script>alert(1)</script><embed src=\"b.swf\"/></iframe></object>",
        "<iframe src=\"x\"><object data=\"y\"><script>alert(2)</script></object></iframe>",
        "<p>Safe Content</p>",
        "</div>"
    );
    let sanitized = XhtmlConformance::sanitize(nested);
    assert_eq!(sanitized, "<div><p>Safe Content</p></div>");
    assert!(XhtmlValidator::validate(&sanitized).is_ok());
}

#[test]
fn appsec04_sanitize_blocks_data_image_svg_xml_in_href_and_uri_attrs() {
    let cases = [
        (
            r#"<a href="data:image/svg+xml;base64,PHN2ZyBvbmxvYWQ9YWxlcnQoMSk+PC9zdmc+">SVG</a>"#,
            r#"<a>SVG</a>"#,
        ),
        (
            r#"<a href="  DaTa:&#x09;image/svg+xml,&lt;svg onload=alert(1)/&gt;">SVG2</a>"#,
            r#"<a>SVG2</a>"#,
        ),
        (
            r#"<img src="data:image/svg+xml;utf8,&lt;svg&gt;&lt;/svg&gt;" alt="bad" />"#,
            r#"<img alt="bad" />"#,
        ),
    ];

    for (input, expected) in cases {
        let sanitized = XhtmlConformance::sanitize(input);
        assert_eq!(sanitized, expected, "failed on: {input}");
        assert!(XhtmlValidator::validate(&sanitized).is_ok());
    }
}
