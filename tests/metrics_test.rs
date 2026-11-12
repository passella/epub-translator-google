use epub_translator_google::domain::entities::{ChapterLatency, ExecutionMetrics};
use std::time::Duration;

#[test]
fn test_execution_metrics_empty_defaults() {
    let metrics = ExecutionMetrics::new();

    assert_eq!(metrics.total_chapters, 0);
    assert_eq!(metrics.cache_hits, 0);
    assert_eq!(metrics.cache_misses, 0);
    assert_eq!(metrics.empty_chapters, 0);
    assert!(metrics.chapter_latencies.is_empty());
    assert_eq!(metrics.total_elapsed, Duration::ZERO);
    assert_eq!(metrics.cache_hit_ratio(), 0.0);
    assert_eq!(metrics.avg_provider_latency(), Duration::ZERO);
    assert_eq!(metrics.min_provider_latency(), None);
    assert_eq!(metrics.max_provider_latency(), None);
}

#[test]
fn test_execution_metrics_with_only_empty_chapters() {
    let metrics = ExecutionMetrics {
        total_chapters: 5,
        cache_hits: 0,
        cache_misses: 0,
        empty_chapters: 5,
        chapter_latencies: Vec::new(),
        total_elapsed: Duration::from_millis(50),
    };

    assert_eq!(metrics.cache_hit_ratio(), 0.0);
    assert_eq!(metrics.avg_provider_latency(), Duration::ZERO);
    assert_eq!(metrics.min_provider_latency(), None);
    assert_eq!(metrics.max_provider_latency(), None);
}

#[test]
fn test_execution_metrics_100_percent_cache_hits() {
    let metrics = ExecutionMetrics {
        total_chapters: 4,
        cache_hits: 4,
        cache_misses: 0,
        empty_chapters: 0,
        chapter_latencies: Vec::new(),
        total_elapsed: Duration::from_millis(120),
    };

    assert_eq!(metrics.cache_hit_ratio(), 100.0);
    assert_eq!(metrics.avg_provider_latency(), Duration::ZERO);
    assert_eq!(metrics.min_provider_latency(), None);
    assert_eq!(metrics.max_provider_latency(), None);
}

#[test]
fn test_execution_metrics_100_percent_cache_misses_single_item() {
    let latency = Duration::from_millis(1500);
    let metrics = ExecutionMetrics {
        total_chapters: 1,
        cache_hits: 0,
        cache_misses: 1,
        empty_chapters: 0,
        chapter_latencies: vec![ChapterLatency {
            index: 0,
            path: "OEBPS/ch1.xhtml".to_string(),
            latency,
        }],
        total_elapsed: Duration::from_millis(1550),
    };

    assert_eq!(metrics.cache_hit_ratio(), 0.0);
    assert_eq!(metrics.avg_provider_latency(), latency);
    assert_eq!(metrics.min_provider_latency(), Some(latency));
    assert_eq!(metrics.max_provider_latency(), Some(latency));
}

#[test]
fn test_execution_metrics_mixed_hits_and_misses() {
    let latencies = vec![
        ChapterLatency {
            index: 1,
            path: "OEBPS/ch2.xhtml".to_string(),
            latency: Duration::from_millis(1000),
        },
        ChapterLatency {
            index: 3,
            path: "OEBPS/ch4.xhtml".to_string(),
            latency: Duration::from_millis(2000),
        },
        ChapterLatency {
            index: 4,
            path: "OEBPS/ch5.xhtml".to_string(),
            latency: Duration::from_millis(3000),
        },
    ];

    let metrics = ExecutionMetrics {
        total_chapters: 6,
        cache_hits: 3,
        cache_misses: 3,
        empty_chapters: 0,
        chapter_latencies: latencies,
        total_elapsed: Duration::from_millis(6100),
    };

    assert_eq!(metrics.cache_hit_ratio(), 50.0);
    assert_eq!(metrics.avg_provider_latency(), Duration::from_millis(2000));
    assert_eq!(
        metrics.min_provider_latency(),
        Some(Duration::from_millis(1000))
    );
    assert_eq!(
        metrics.max_provider_latency(),
        Some(Duration::from_millis(3000))
    );
}

#[test]
fn test_execution_metrics_fractional_ratio_and_precision() {
    let metrics = ExecutionMetrics {
        total_chapters: 3,
        cache_hits: 1,
        cache_misses: 2,
        empty_chapters: 0,
        chapter_latencies: vec![
            ChapterLatency {
                index: 0,
                path: "OEBPS/ch1.xhtml".to_string(),
                latency: Duration::from_micros(100),
            },
            ChapterLatency {
                index: 1,
                path: "OEBPS/ch2.xhtml".to_string(),
                latency: Duration::from_micros(300),
            },
        ],
        total_elapsed: Duration::from_millis(10),
    };

    let ratio = metrics.cache_hit_ratio();
    assert!((ratio - 33.333333333333336).abs() < 1e-9);
    assert_eq!(metrics.avg_provider_latency(), Duration::from_micros(200));
}
