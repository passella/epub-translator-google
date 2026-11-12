use epub_translator_google::adapters::cache::SqliteCacheAdapter;
use epub_translator_google::adapters::cli::InteractivePrompter;
use epub_translator_google::adapters::translator::{
    AntigravityTranslatorAdapter, RetryConfig, calculate_backoff, is_transient_error,
};
use epub_translator_google::domain::entities::Language;
use epub_translator_google::domain::ports::{TranslationCachePort, TranslatorPort};
use epub_translator_google::domain::services::XhtmlPartitioner;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static SCRIPT_COUNTER: AtomicU64 = AtomicU64::new(100);

fn unique_temp_file(prefix: &str) -> PathBuf {
    let id = SCRIPT_COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!("{}_{}_{}.txt", prefix, std::process::id(), id))
}

fn create_mock_script(content: &str, file_name: &str) -> PathBuf {
    let id = SCRIPT_COUNTER.fetch_add(1, Ordering::SeqCst);
    let unique_name = format!("{}_{}_{}", std::process::id(), id, file_name);
    let script_path = std::env::temp_dir().join(unique_name);
    fs::write(&script_path, content).unwrap();
    let mut perms = fs::metadata(&script_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).unwrap();
    std::thread::sleep(Duration::from_millis(5));
    script_path
}

#[test]
fn test_antigravity_model_name_configuration() {
    let default_adapter = AntigravityTranslatorAdapter::new(None);
    assert_eq!(default_adapter.model_name(), None);
    assert_eq!(default_adapter.provider_name(), "antigravity");

    let custom_adapter =
        AntigravityTranslatorAdapter::new(Some("gemini-4-argon-medium".to_string()));
    assert_eq!(custom_adapter.model_name(), Some("gemini-4-argon-medium"));
    assert_eq!(custom_adapter.provider_name(), "antigravity");
}

#[test]
fn test_fetch_available_models_with_non_existent_binary() {
    let dummy_path = Path::new("/tmp/non_existent_agy_binary_xyz123");
    let models = AntigravityTranslatorAdapter::fetch_available_models(dummy_path);
    assert!(models.is_empty());
}

#[test]
fn test_interactive_prompter_prompt_model_with_provided_argument() {
    let res_default = InteractivePrompter::prompt_model(Some("default".to_string())).unwrap();
    assert_eq!(res_default, None);

    let res_system_default =
        InteractivePrompter::prompt_model(Some("[Padrão do Sistema]".to_string())).unwrap();
    assert_eq!(res_system_default, None);

    let res_empty = InteractivePrompter::prompt_model(Some("   ".to_string())).unwrap();
    assert_eq!(res_empty, None);

    let res_custom =
        InteractivePrompter::prompt_model(Some("gemini-3.8-flash-high".to_string())).unwrap();
    assert_eq!(res_custom, Some("gemini-3.8-flash-high".to_string()));
}

#[test]
fn test_is_transient_error_negative_cases() {
    assert!(!is_transient_error("", ""));
    assert!(!is_transient_error("normal stdout", ""));
    assert!(!is_transient_error("", "Unknown option: --foo"));
    assert!(!is_transient_error("", "SyntaxError: Unexpected token"));
    assert!(!is_transient_error("Invalid argument", ""));
}

#[test]
fn test_is_transient_error_positive_cases() {
    assert!(is_transient_error("", "500 Internal Server Error"));
    assert!(is_transient_error("Eligibility check failed", ""));
    assert!(is_transient_error("", "Network unreachable"));
    assert!(is_transient_error("", "Connection reset by peer"));
    assert!(is_transient_error("", "Connection refused"));
    assert!(is_transient_error("Request timed out", ""));
    assert!(is_transient_error("", "Rate limit exceeded"));
    assert!(is_transient_error("", "HTTP 429 Too Many Requests"));
    assert!(is_transient_error("", "502 Bad Gateway"));
    assert!(is_transient_error("", "503 Service Unavailable"));
    assert!(is_transient_error("", "504 Gateway Timeout"));
    assert!(is_transient_error(
        "",
        "Temporary failure in name resolution"
    ));
    assert!(is_transient_error("", "broken pipe"));
}

#[test]
fn test_calculate_backoff_values_and_jitter() {
    let initial = Duration::from_millis(100);
    let max = Duration::from_millis(500);

    let d0 = calculate_backoff(0, initial, max, 2.0);
    assert!(d0 >= Duration::from_millis(100));
    assert!(d0 <= Duration::from_millis(200));

    let d1 = calculate_backoff(1, initial, max, 2.0);
    assert!(d1 >= Duration::from_millis(200));

    let d5 = calculate_backoff(5, initial, max, 2.0);
    assert!(d5 >= Duration::from_millis(500));
    assert!(d5 <= Duration::from_millis(1000));
}

#[tokio::test]
async fn test_antigravity_retry_on_transient_failure_then_success() {
    let counter_file = unique_temp_file("agy_test_counter_1");
    if counter_file.exists() {
        let _ = fs::remove_file(&counter_file);
    }
    let script_content = format!(
        "#!/bin/sh\nCOUNT=0\nif [ -f \"{}\" ]; then\nCOUNT=$(cat \"{}\")\nfi\nCOUNT=$((COUNT + 1))\necho \"$COUNT\" > \"{}\"\nif [ \"$COUNT\" -eq 1 ]; then\necho \"500 Internal Server Error: Eligibility check failed\" >&2\nexit 1\nfi\necho '{{\"status\":\"ok\",\"response\":\"<p>Traduzido</p>\"}}'\nexit 0\n",
        counter_file.display(),
        counter_file.display(),
        counter_file.display()
    );
    let script_path = create_mock_script(&script_content, "mock_agy_transient_success.sh");

    let adapter = AntigravityTranslatorAdapter::with_binary_path(&script_path, None)
        .with_retry_config(RetryConfig {
            max_attempts: 3,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(50),
            backoff_multiplier: 2.0,
        });

    let res = adapter
        .translate_html(
            "<p>Hello</p>",
            &Language::new("en"),
            &Language::new("pt-BR"),
        )
        .await
        .unwrap();

    assert_eq!(res.translated_text, "<p>Traduzido</p>");
    let recorded_count = fs::read_to_string(&counter_file).unwrap();
    assert_eq!(recorded_count.trim(), "2");

    let _ = fs::remove_file(script_path);
    let _ = fs::remove_file(counter_file);
}

#[tokio::test]
async fn test_antigravity_non_transient_error_fails_immediately() {
    let counter_file = unique_temp_file("agy_test_counter_2");
    if counter_file.exists() {
        let _ = fs::remove_file(&counter_file);
    }
    let script_content = format!(
        "#!/bin/sh\nCOUNT=0\nif [ -f \"{}\" ]; then\nCOUNT=$(cat \"{}\")\nfi\nCOUNT=$((COUNT + 1))\necho \"$COUNT\" > \"{}\"\necho \"Unknown option --invalid-flag\" >&2\nexit 1\n",
        counter_file.display(),
        counter_file.display(),
        counter_file.display()
    );
    let script_path = create_mock_script(&script_content, "mock_agy_non_transient.sh");

    let adapter = AntigravityTranslatorAdapter::with_binary_path(&script_path, None)
        .with_retry_config(RetryConfig {
            max_attempts: 3,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(50),
            backoff_multiplier: 2.0,
        });

    let res = adapter
        .translate_html(
            "<p>Hello</p>",
            &Language::new("en"),
            &Language::new("pt-BR"),
        )
        .await;

    assert!(res.is_err());
    let recorded_count = fs::read_to_string(&counter_file).unwrap();
    assert_eq!(recorded_count.trim(), "1");

    let _ = fs::remove_file(script_path);
    let _ = fs::remove_file(counter_file);
}

#[tokio::test]
async fn test_antigravity_transient_error_exhausts_retries() {
    let counter_file = unique_temp_file("agy_test_counter_3");
    if counter_file.exists() {
        let _ = fs::remove_file(&counter_file);
    }
    let script_content = format!(
        "#!/bin/sh\nCOUNT=0\nif [ -f \"{}\" ]; then\nCOUNT=$(cat \"{}\")\nfi\nCOUNT=$((COUNT + 1))\necho \"$COUNT\" > \"{}\"\necho \"500 Internal Server Error\" >&2\nexit 1\n",
        counter_file.display(),
        counter_file.display(),
        counter_file.display()
    );
    let script_path = create_mock_script(&script_content, "mock_agy_transient_exhaust.sh");

    let adapter = AntigravityTranslatorAdapter::with_binary_path(&script_path, None)
        .with_retry_config(RetryConfig {
            max_attempts: 3,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(50),
            backoff_multiplier: 2.0,
        });

    let res = adapter
        .translate_html(
            "<p>Hello</p>",
            &Language::new("en"),
            &Language::new("pt-BR"),
        )
        .await;

    assert!(res.is_err());
    let recorded_count = fs::read_to_string(&counter_file).unwrap();
    assert_eq!(recorded_count.trim(), "3");

    let _ = fs::remove_file(script_path);
    let _ = fs::remove_file(counter_file);
}

#[tokio::test]
async fn test_antigravity_adaptive_partitioning_giant_chapter() {
    let call_counter = unique_temp_file("agy_partition_counter");
    if call_counter.exists() {
        let _ = fs::remove_file(&call_counter);
    }

    let script_content = format!(
        r#"#!/bin/sh
COUNT=0
if [ -f "{}" ]; then
  COUNT=$(cat "{}")
fi
COUNT=$((COUNT + 1))
echo "$COUNT" > "{}"

PROMPT=$(cat)
BODY=$(echo "$PROMPT" | awk '/DOCUMENT CONTENT TO TRANSLATE:/ {{flag=1; next}} flag')

TRANSLATED=$(echo "$BODY" | sed 's/Hello/Olá/g; s/World/Mundo/g; s/Story/História/g')

printf '{{"status":"ok","response":"%s"}}\n' "$(echo "$TRANSLATED" | tr '\n' ' ')"
exit 0
"#,
        call_counter.display(),
        call_counter.display(),
        call_counter.display()
    );

    let script_path = create_mock_script(&script_content, "mock_agy_partition.sh");

    let giant_html = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE html>
<html>
<head><title>Original</title></head>
<body>
  <div class="main">
    <p>Hello Story 1</p>
    <p>World Story 2</p>
    <p>Hello World 3</p>
  </div>
</body>
</html>"#;

    let adapter = AntigravityTranslatorAdapter::with_binary_path(&script_path, None)
        .with_partition_threshold(60);

    let res = adapter
        .translate_html(giant_html, &Language::new("en"), &Language::new("pt-BR"))
        .await
        .unwrap();

    let calls = fs::read_to_string(&call_counter).unwrap();
    let num_calls: usize = calls.trim().parse().unwrap();
    assert!(num_calls >= 2);

    assert!(res.translated_text.contains("<title>Original</title>"));
    assert!(res.translated_text.contains("<div class=\"main\">"));
    assert!(res.translated_text.contains("Olá"));
    assert!(res.translated_text.contains("Mundo"));
    assert!(res.translated_text.contains("História"));
    assert!(res.translated_text.ends_with("</div>\n</body>\n</html>"));

    let _ = fs::remove_file(script_path);
    let _ = fs::remove_file(call_counter);
}

#[tokio::test]
async fn test_antigravity_small_chapter_single_block_execution() {
    let call_counter = unique_temp_file("agy_small_counter");
    if call_counter.exists() {
        let _ = fs::remove_file(&call_counter);
    }

    let script_content = format!(
        r#"#!/bin/sh
COUNT=0
if [ -f "{}" ]; then
  COUNT=$(cat "{}")
fi
COUNT=$((COUNT + 1))
echo "$COUNT" > "{}"

echo '{{"status":"ok","response":"<p>Texto</p>"}}'
exit 0
"#,
        call_counter.display(),
        call_counter.display(),
        call_counter.display()
    );

    let script_path = create_mock_script(&script_content, "mock_agy_small.sh");

    let small_html = "<p>Text</p>";
    let adapter = AntigravityTranslatorAdapter::with_binary_path(&script_path, None)
        .with_partition_threshold(1000);

    let res = adapter
        .translate_html(small_html, &Language::new("en"), &Language::new("pt-BR"))
        .await
        .unwrap();

    assert_eq!(res.translated_text, "<p>Texto</p>");
    let calls = fs::read_to_string(&call_counter).unwrap();
    assert_eq!(calls.trim(), "1");

    let _ = fs::remove_file(script_path);
    let _ = fs::remove_file(call_counter);
}

#[tokio::test]
async fn test_antigravity_block_cache_instant_hit_bypasses_binary() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join("test_agy_cache_hit.db");
    let call_counter = unique_temp_file("agy_cache_hit_counter");
    if db_path.exists() {
        let _ = fs::remove_file(&db_path);
    }
    if call_counter.exists() {
        let _ = fs::remove_file(&call_counter);
    }

    let script_content = format!(
        r#"#!/bin/sh
COUNT=0
if [ -f "{}" ]; then
  COUNT=$(cat "{}")
fi
COUNT=$((COUNT + 1))
echo "$COUNT" > "{}"
echo '{{"status":"ok","response":"<p>Unwanted binary call</p>"}}'
exit 0
"#,
        call_counter.display(),
        call_counter.display(),
        call_counter.display()
    );
    let script_path = create_mock_script(&script_content, "mock_agy_instant_hit.sh");

    let cache: Arc<dyn TranslationCachePort> = Arc::new(SqliteCacheAdapter::new(&db_path).unwrap());

    let html = r#"<!DOCTYPE html>
<html>
<body>
  <div class="content">
    <p>First distinct paragraph with sufficient length</p>
    <p>Second distinct paragraph with sufficient length</p>
  </div>
</body>
</html>"#;

    let threshold = 30;
    let partitioner = XhtmlPartitioner::with_threshold(threshold);
    let plan = partitioner.partition(html);
    let chunks = plan.translatable_chunks();
    assert!(chunks.len() >= 2);

    for (i, chunk) in chunks.iter().enumerate() {
        cache
            .set_block(
                "en",
                "pt-BR",
                "antigravity",
                chunk,
                &format!("<p>Bloco {i} em cache</p>"),
            )
            .await
            .unwrap();
    }

    let adapter = AntigravityTranslatorAdapter::with_binary_path(&script_path, None)
        .with_partition_threshold(threshold)
        .with_cache(cache);

    let res = adapter
        .translate_html(html, &Language::new("en"), &Language::new("pt-BR"))
        .await
        .unwrap();

    assert!(!call_counter.exists());
    assert!(res.translated_text.contains("<p>Bloco 0 em cache</p>"));
    assert!(res.translated_text.contains("<p>Bloco 1 em cache</p>"));

    let _ = fs::remove_file(&script_path);
    let _ = fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_antigravity_block_cache_partial_hits_translates_only_missing_blocks() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join("test_agy_partial_cache.db");
    let call_counter = unique_temp_file("agy_partial_cache_counter");
    if db_path.exists() {
        let _ = fs::remove_file(&db_path);
    }
    if call_counter.exists() {
        let _ = fs::remove_file(&call_counter);
    }

    let script_content = format!(
        r#"#!/bin/sh
COUNT=0
if [ -f "{}" ]; then
  COUNT=$(cat "{}")
fi
COUNT=$((COUNT + 1))
echo "$COUNT" > "{}"

PROMPT=$(cat)
BODY=$(echo "$PROMPT" | awk '/DOCUMENT CONTENT TO TRANSLATE:/ {{flag=1; next}} flag')
TRANSLATED=$(echo "$BODY" | sed 's/Second/Segundo traduzido dinamicamente/g')
printf '{{"status":"ok","response":"%s"}}\n' "$(echo "$TRANSLATED" | tr '\n' ' ')"
exit 0
"#,
        call_counter.display(),
        call_counter.display(),
        call_counter.display()
    );
    let script_path = create_mock_script(&script_content, "mock_agy_partial_cache.sh");

    let cache: Arc<dyn TranslationCachePort> = Arc::new(SqliteCacheAdapter::new(&db_path).unwrap());

    let html = r#"<!DOCTYPE html>
<html>
<body>
  <div class="content">
    <p>First distinct block with sufficient length</p>
    <p>Second distinct block with sufficient length</p>
    <p>Third distinct block with sufficient length</p>
  </div>
</body>
</html>"#;

    let threshold = 30;
    let partitioner = XhtmlPartitioner::with_threshold(threshold);
    let plan = partitioner.partition(html);
    let chunks = plan.translatable_chunks();
    assert!(chunks.len() >= 3);

    cache
        .set_block(
            "en",
            "pt-BR",
            "antigravity",
            chunks[0],
            "<p>Primeiro bloco em cache</p>",
        )
        .await
        .unwrap();

    cache
        .set_block(
            "en",
            "pt-BR",
            "antigravity",
            chunks[2],
            "<p>Terceiro bloco em cache</p>",
        )
        .await
        .unwrap();

    let adapter = AntigravityTranslatorAdapter::with_binary_path(&script_path, None)
        .with_partition_threshold(threshold)
        .with_cache(cache.clone());

    let res = adapter
        .translate_html(html, &Language::new("en"), &Language::new("pt-BR"))
        .await
        .unwrap();

    let count_str = fs::read_to_string(&call_counter).unwrap();
    assert_eq!(count_str.trim(), "1");

    assert!(
        res.translated_text
            .contains("<p>Primeiro bloco em cache</p>")
    );
    assert!(
        res.translated_text
            .contains("Segundo traduzido dinamicamente")
    );
    assert!(
        res.translated_text
            .contains("<p>Terceiro bloco em cache</p>")
    );

    let pos1 = res.translated_text.find("Primeiro bloco em cache").unwrap();
    let pos2 = res
        .translated_text
        .find("Segundo traduzido dinamicamente")
        .unwrap();
    let pos3 = res.translated_text.find("Terceiro bloco em cache").unwrap();
    assert!(pos1 < pos2);
    assert!(pos2 < pos3);

    let newly_cached = cache
        .get_block("en", "pt-BR", "antigravity", chunks[1])
        .await
        .unwrap();
    assert!(newly_cached.is_some());
    assert!(
        newly_cached
            .unwrap()
            .contains("Segundo traduzido dinamicamente")
    );

    let _ = fs::remove_file(&script_path);
    let _ = fs::remove_file(&call_counter);
    let _ = fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_antigravity_parallel_block_concurrency_bounded_and_ordered() {
    let temp_dir = std::env::temp_dir();
    let active_dir = temp_dir.join("agy_concurrent_pids");
    let max_concurrency_file = temp_dir.join("agy_max_concurrency.txt");
    if active_dir.exists() {
        let _ = fs::remove_dir_all(&active_dir);
    }
    let _ = fs::create_dir_all(&active_dir);
    let _ = fs::write(&max_concurrency_file, "0");

    let script_content = format!(
        r#"#!/bin/sh
PID=$$
ACTIVE_DIR="{}"
MAX_FILE="{}"

touch "$ACTIVE_DIR/$PID"
COUNT=$(ls -1 "$ACTIVE_DIR" | wc -l)

CURRENT_MAX=0
if [ -f "$MAX_FILE" ]; then
  CURRENT_MAX=$(cat "$MAX_FILE")
fi
if [ "$COUNT" -gt "$CURRENT_MAX" ]; then
  echo "$COUNT" > "$MAX_FILE"
fi

sleep 0.05
rm -f "$ACTIVE_DIR/$PID"

PROMPT=$(cat)
BODY=$(echo "$PROMPT" | awk '/DOCUMENT CONTENT TO TRANSLATE:/ {{flag=1; next}} flag')
TRANSLATED=$(echo "$BODY" | sed 's/Section/Secao/g')
printf '{{"status":"ok","response":"%s"}}\n' "$(echo "$TRANSLATED" | tr '\n' ' ')"
exit 0
"#,
        active_dir.display(),
        max_concurrency_file.display()
    );
    let script_path = create_mock_script(&script_content, "mock_agy_concurrency.sh");

    let html = r#"<!DOCTYPE html>
<html>
<body>
  <div class="content">
    <p>Section Alpha</p>
    <p>Section Beta</p>
    <p>Section Gamma</p>
    <p>Section Delta</p>
  </div>
</body>
</html>"#;

    let adapter = AntigravityTranslatorAdapter::with_binary_path(&script_path, None)
        .with_partition_threshold(50)
        .with_block_concurrency(2);

    assert_eq!(adapter.block_concurrency(), 2);

    let res = adapter
        .translate_html(html, &Language::new("en"), &Language::new("pt-BR"))
        .await
        .unwrap();

    assert!(res.translated_text.contains("Secao Alpha"));
    assert!(res.translated_text.contains("Secao Beta"));
    assert!(res.translated_text.contains("Secao Gamma"));
    assert!(res.translated_text.contains("Secao Delta"));

    let pos_a = res.translated_text.find("Secao Alpha").unwrap();
    let pos_b = res.translated_text.find("Secao Beta").unwrap();
    let pos_c = res.translated_text.find("Secao Gamma").unwrap();
    let pos_d = res.translated_text.find("Secao Delta").unwrap();
    assert!(pos_a < pos_b);
    assert!(pos_b < pos_c);
    assert!(pos_c < pos_d);

    let recorded_max: usize = fs::read_to_string(&max_concurrency_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap_or(0);
    assert!(recorded_max <= 2);

    let _ = fs::remove_file(&script_path);
    let _ = fs::remove_dir_all(&active_dir);
    let _ = fs::remove_file(&max_concurrency_file);
}

#[tokio::test]
async fn test_antigravity_parallel_block_failure_aborts_tasks() {
    let script_content = r#"#!/bin/sh
PROMPT=$(cat)
if echo "$PROMPT" | grep -q "FailingSection"; then
  echo "Fatal error in block translation" >&2
  exit 1
fi
echo '{"status":"ok","response":"<p>Success</p>"}'
exit 0
"#;
    let script_path = create_mock_script(script_content, "mock_agy_block_fail.sh");

    let html = r#"<!DOCTYPE html>
<html>
<body>
  <div class="content">
    <p>NormalSection 1</p>
    <p>FailingSection 2</p>
    <p>NormalSection 3</p>
  </div>
</body>
</html>"#;

    let adapter = AntigravityTranslatorAdapter::with_binary_path(&script_path, None)
        .with_partition_threshold(50)
        .with_block_concurrency(2);

    let res = adapter
        .translate_html(html, &Language::new("en"), &Language::new("pt-BR"))
        .await;

    assert!(res.is_err());

    let _ = fs::remove_file(&script_path);
}
