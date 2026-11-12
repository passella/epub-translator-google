use crate::domain::entities::ExecutionMetrics;

pub trait ProgressReporterPort: Send + Sync {
    fn on_start(&self, total_chapters: usize, book_title: &str);
    fn on_chapter_start(&self, index: usize, chapter_path: &str);
    fn on_chapter_complete(&self, index: usize, chapter_path: &str);
    fn on_finish(&self, message: &str);
    fn on_metrics(&self, _metrics: &ExecutionMetrics) {}
}
