use epub_translator_google::adapters::translator::google_free::{
    GoogleFreeTranslatorAdapter, split_into_safe_chunks,
};

#[test]
fn test_split_into_safe_chunks_empty_and_small() {
    let empty = "";
    assert!(split_into_safe_chunks(empty, 1800).is_empty());

    let small = "Hello world!";
    let chunks = split_into_safe_chunks(small, 1800);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0], small);
}

#[test]
fn test_split_into_safe_chunks_exact_boundary() {
    let exact = "a".repeat(1800);
    let chunks = split_into_safe_chunks(&exact, 1800);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0], exact);

    let slightly_over = "a".repeat(1801);
    let chunks_over = split_into_safe_chunks(&slightly_over, 1800);
    assert_eq!(chunks_over.len(), 2);
    assert_eq!(chunks_over.concat(), slightly_over);
    for chunk in &chunks_over {
        assert!(chunk.len() <= 1800);
    }
}

#[test]
fn test_split_into_safe_chunks_paragraphs_and_lines() {
    let mut text = String::new();
    for i in 0..50 {
        text.push_str(&format!("This is line number {i} with some content.\n"));
    }
    let chunks = split_into_safe_chunks(&text, 500);
    assert!(chunks.len() > 1);
    assert_eq!(chunks.concat(), text);
    for chunk in &chunks {
        assert!(chunk.len() <= 500);
    }
}

#[test]
fn test_split_into_safe_chunks_preserves_placeholders() {
    let mut text = String::new();
    for i in 0..100 {
        text.push_str(&format!(
            "Text before ⟦{i}⟧ text inside tag boundary ⟦{i}⟧ text after.\n"
        ));
    }
    let chunks = split_into_safe_chunks(&text, 600);
    assert!(chunks.len() > 1);
    assert_eq!(chunks.concat(), text);

    for chunk in &chunks {
        let open_count = chunk.matches('⟦').count();
        let close_count = chunk.matches('⟧').count();
        assert_eq!(
            open_count, close_count,
            "Chunk broke placeholder balance: {chunk}"
        );
    }
}

#[test]
fn test_split_into_safe_chunks_multibyte_utf8() {
    let mut text = String::new();
    for _ in 0..200 {
        text.push_str("Olá mundo! こんにちは世界! 🚀 Teste com emojis e acentuação.\n");
    }
    let chunks = split_into_safe_chunks(&text, 700);
    assert!(chunks.len() > 1);
    assert_eq!(chunks.concat(), text);
    for chunk in &chunks {
        assert!(chunk.len() <= 700);
    }
}

#[test]
fn test_html_masking_and_unmasking_cycle() {
    let adapter = GoogleFreeTranslatorAdapter::new().unwrap();
    let original_html = "<h1>contents</h1>\n<div class=\"readable-text\">\n<p><a href=\"../Text/preface.html\"><em>preface</em></a></p>\n</div>";
    let (masked, tags) = adapter.mask_and_translate_html_block(original_html);

    assert!(masked.contains("contents"));
    assert!(masked.contains("preface"));
    assert!(!masked.contains("<h1>"));
    assert!(!masked.contains("</a>"));

    let unmasked = GoogleFreeTranslatorAdapter::unmask_html(&masked, &tags);
    assert_eq!(unmasked, original_html);
}

#[test]
fn test_html_unmasking_with_spaced_placeholders_and_double_digits() {
    let mut tags = Vec::new();
    for i in 0..15 {
        tags.push(format!("<tag_{i}>"));
    }

    let masked_with_spaces = "Text ⟦0⟧ text ⟦ 1 ⟧ text ⟦10⟧ text ⟦ 11 ⟧ text ⟦ 14⟧ end.";
    let unmasked = GoogleFreeTranslatorAdapter::unmask_html(masked_with_spaces, &tags);

    assert!(unmasked.contains("<tag_0>"));
    assert!(unmasked.contains("<tag_1>"));
    assert!(unmasked.contains("<tag_10>"));
    assert!(unmasked.contains("<tag_11>"));
    assert!(unmasked.contains("<tag_14>"));
    assert!(!unmasked.contains("⟦"));
    assert!(!unmasked.contains("⟧"));
}

#[test]
fn test_unmask_html_orphan_detection_negative_happy_path() {
    let tags = vec!["<h1>".to_string(), "</h1>".to_string()];
    let masked = "⟦0⟧Título Traduzido⟦1⟧";
    let unmasked = GoogleFreeTranslatorAdapter::unmask_html(masked, &tags);
    assert!(!GoogleFreeTranslatorAdapter::has_orphan_placeholders(
        &unmasked
    ));
    assert!(GoogleFreeTranslatorAdapter::find_orphan_placeholders(&unmasked).is_empty());
    assert_eq!(unmasked, "<h1>Título Traduzido</h1>");
}

#[test]
fn test_unmask_html_orphan_detection_positive_missing_or_corrupted_tag() {
    let tags = vec!["<h1>".to_string()];
    let masked = "⟦0⟧Título Traduzido⟦1⟧ e mais texto ⟦ 2 ⟧";
    let unmasked = GoogleFreeTranslatorAdapter::unmask_html(masked, &tags);
    assert!(GoogleFreeTranslatorAdapter::has_orphan_placeholders(
        &unmasked
    ));
    let orphans = GoogleFreeTranslatorAdapter::find_orphan_placeholders(&unmasked);
    assert_eq!(orphans.len(), 2);
    assert!(orphans.contains(&"⟦1⟧".to_string()));
    assert!(orphans.contains(&"⟦ 2 ⟧".to_string()));
}

#[test]
fn test_normalize_placeholders_variations_and_non_placeholders() {
    let input_double_brackets = "Texto [[0]] e [[ 1 ]] e [[2 ]]";
    assert_eq!(
        GoogleFreeTranslatorAdapter::normalize_placeholders(input_double_brackets),
        "Texto ⟦0⟧ e ⟦1⟧ e ⟦2⟧"
    );

    let input_spaced_brackets = "Texto [ 0 ] e [ 1] e [2 ]";
    assert_eq!(
        GoogleFreeTranslatorAdapter::normalize_placeholders(input_spaced_brackets),
        "Texto ⟦0⟧ e ⟦1⟧ e ⟦2⟧"
    );

    let input_single_brackets = "Texto [0] e [1] e [42]";
    assert_eq!(
        GoogleFreeTranslatorAdapter::normalize_placeholders(input_single_brackets),
        "Texto ⟦0⟧ e ⟦1⟧ e ⟦42⟧"
    );

    let input_spaced_math_brackets = "Texto ⟦ 0 ⟧ e ⟦ 1⟧ e ⟦2 ⟧";
    assert_eq!(
        GoogleFreeTranslatorAdapter::normalize_placeholders(input_spaced_math_brackets),
        "Texto ⟦0⟧ e ⟦1⟧ e ⟦2⟧"
    );

    let input_mixed = "Prefácio [[0]] título [ 1 ] corpo [2] notas ⟦ 3 ⟧ fim";
    assert_eq!(
        GoogleFreeTranslatorAdapter::normalize_placeholders(input_mixed),
        "Prefácio ⟦0⟧ título ⟦1⟧ corpo ⟦2⟧ notas ⟦3⟧ fim"
    );

    let input_non_placeholders = "Link [nota 1] e [teste] e [[wiki]] e [] e [-1]";
    assert_eq!(
        GoogleFreeTranslatorAdapter::normalize_placeholders(input_non_placeholders),
        "Link [nota 1] e [teste] e [[wiki]] e [] e [-1]"
    );
}

#[test]
fn test_unmask_html_with_corrupted_brackets_normalization_cycle() {
    let tags = vec![
        "<h1>".to_string(),
        "</h1>".to_string(),
        "<p>".to_string(),
        "</p>".to_string(),
    ];
    let corrupted_translated = "[[0]]Capítulo 1[[1]]\n[ 2 ]Texto do capítulo.[3]";
    let normalized = GoogleFreeTranslatorAdapter::normalize_placeholders(corrupted_translated);
    let unmasked = GoogleFreeTranslatorAdapter::unmask_html(&normalized, &tags);

    assert_eq!(unmasked, "<h1>Capítulo 1</h1>\n<p>Texto do capítulo.</p>");
    assert!(!GoogleFreeTranslatorAdapter::has_orphan_placeholders(
        &unmasked
    ));
    assert!(GoogleFreeTranslatorAdapter::find_orphan_placeholders(&unmasked).is_empty());
}

#[test]
fn test_unmask_html_with_orphan_corrupted_brackets_detected() {
    let tags = vec!["<h1>".to_string(), "</h1>".to_string()];
    let corrupted_with_extra = "[[0]]Capítulo 1[[1]] com resíduo [[99]] e [ 100 ]";
    let normalized = GoogleFreeTranslatorAdapter::normalize_placeholders(corrupted_with_extra);
    let unmasked = GoogleFreeTranslatorAdapter::unmask_html(&normalized, &tags);
    let normalized_unmasked = GoogleFreeTranslatorAdapter::normalize_placeholders(&unmasked);

    assert!(GoogleFreeTranslatorAdapter::has_orphan_placeholders(
        &normalized_unmasked
    ));
    let orphans = GoogleFreeTranslatorAdapter::find_orphan_placeholders(&normalized_unmasked);
    assert_eq!(orphans.len(), 2);
    assert!(orphans.contains(&"⟦99⟧".to_string()));
    assert!(orphans.contains(&"⟦100⟧".to_string()));
}
