use epub_translator_google::domain::services::XhtmlPartitioner;

#[test]
fn test_fuzz_malformed_and_broken_brackets() {
    let partitioner = XhtmlPartitioner::with_threshold(100);
    let inputs = [
        "<<< >>> <><><<",
        "< 123 > < / 123 >",
        "<p class=\"unclosed>Sem fechamento de aspas",
        "<p class='unclosed>Outro sem fechamento",
        "<div id=\"test\" class=\"",
        "<a href=>Link quebrado</a>",
        "<><><p></p></><>",
        "<!----> <!-- unclosed comment",
        "<![CDATA[ unclosed cdata",
        "<?xml unclosed pi",
        "<!DOCTYPE unclosed doctype",
        "<p>Normal</p><<broken tag>><p>Outro</p>",
    ];

    for input in inputs {
        let plan = partitioner.partition(input);
        let chunks = plan.translatable_chunks();
        let recomposed = plan.recompose(&chunks);
        assert!(!recomposed.is_empty() || input.is_empty());
    }
}

#[test]
fn test_fuzz_unbalanced_and_deeply_nested_tags() {
    let partitioner = XhtmlPartitioner::with_threshold(500);

    let mut deep_open = String::new();
    for i in 0..300 {
        deep_open.push_str(&format!("<div id=\"d{i}\"><span class=\"s{i}\">"));
    }
    deep_open.push_str("Conteúdo profundo no fundo do poço");
    for _ in 0..150 {
        deep_open.push_str("</div>");
    }

    let plan = partitioner.partition(&deep_open);
    let chunks = plan.translatable_chunks();
    let recomposed = plan.recompose(&chunks);
    assert!(recomposed.contains("Conteúdo profundo"));
}

#[test]
fn test_fuzz_interleaved_tags() {
    let partitioner = XhtmlPartitioner::with_threshold(200);
    let interleaved = "<b><i>negrito e itálico misturados</b></i><u>sublinhado</i></u>";
    let plan = partitioner.partition(interleaved);
    let chunks = plan.translatable_chunks();
    let recomposed = plan.recompose(&chunks);
    assert_eq!(recomposed, interleaved);
}

#[test]
fn test_fuzz_control_characters_and_binary_garbage() {
    let partitioner = XhtmlPartitioner::with_threshold(100);
    let mut garbage = String::new();
    for b in 0u8..=31u8 {
        garbage.push(b as char);
    }
    garbage.push_str("<p>Texto com \x00 bytes nulos e \x1F controles</p>");
    garbage.push_str("<div attr=\"\x07\x08\">Unicode: 🦀 日本語 العربية 🌍</div>");

    let plan = partitioner.partition(&garbage);
    let chunks = plan.translatable_chunks();
    let recomposed = plan.recompose(&chunks);
    assert!(recomposed.contains("🦀"));
    assert!(recomposed.contains("日本語"));
    assert!(recomposed.contains("العربية"));
}

#[test]
fn test_fuzz_giant_strings_and_stress_thresholds() {
    let giant_paragraph = format!("<p>{}</p>", "Palavra ".repeat(50_000));
    assert!(giant_paragraph.len() > 300_000);

    let thresholds = [10, 100, 1000, 50_000, 200_000, 500_000];
    for t in thresholds {
        let partitioner = XhtmlPartitioner::with_threshold(t);
        let plan = partitioner.partition(&giant_paragraph);
        let chunks = plan.translatable_chunks();
        let recomposed = plan.recompose(&chunks);
        assert_eq!(recomposed, giant_paragraph);
    }
}

#[test]
fn test_fuzz_massive_sibling_elements() {
    let partitioner = XhtmlPartitioner::with_threshold(1024);
    let mut document = String::from("<html><body>");
    for i in 0..5_000 {
        document.push_str(&format!("<p id=\"item-{i}\">Parágrafo número {i}</p>\n"));
    }
    document.push_str("</body></html>");

    let plan = partitioner.partition(&document);
    let chunks = plan.translatable_chunks();
    assert!(!chunks.is_empty());
    let recomposed = plan.recompose(&chunks);
    assert!(recomposed.contains("Parágrafo número 0"));
    assert!(recomposed.contains("Parágrafo número 4999"));
}

#[test]
fn test_fuzz_giant_attributes_stress() {
    let partitioner = XhtmlPartitioner::with_threshold(1000);
    let giant_attr = "x".repeat(30_000);
    let input = format!("<div data-huge=\"{giant_attr}\"><p>Texto</p></div>");

    let plan = partitioner.partition(&input);
    let chunks = plan.translatable_chunks();
    let recomposed = plan.recompose(&chunks);
    assert_eq!(recomposed, input);
}

#[test]
fn test_fuzz_deterministic_mutation_loop() {
    let mut state: u64 = 987654321;
    let mut next_rand = || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        state
    };

    let base_tags = [
        "<p>",
        "</p>",
        "<div>",
        "</div>",
        "<span>",
        "</span>",
        "<img src=\"x.png\" />",
        "<br>",
        "<hr>",
        " class=\"attr",
        " id='item'",
        " broken=\"",
        "<!-- comment -->",
        "<![CDATA[ data ]]>",
        "&amp;",
        "&",
        "<",
        ">",
        "\"",
        "'",
    ];

    let partitioner = XhtmlPartitioner::with_threshold(150);

    for _ in 0..1_000 {
        let mut fuzz_str = String::with_capacity(512);
        let parts_count = (next_rand() % 30) as usize + 1;
        for _ in 0..parts_count {
            let choice = (next_rand() % base_tags.len() as u64) as usize;
            fuzz_str.push_str(base_tags[choice]);
            if next_rand() % 4 == 0 {
                fuzz_str.push_str(" texto ");
            }
        }

        let plan = partitioner.partition(&fuzz_str);
        let chunks = plan.translatable_chunks();
        let recomposed = plan.recompose(&chunks);
        assert!(!recomposed.is_empty() || fuzz_str.is_empty());
    }
}
