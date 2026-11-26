use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(1);

pub struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    pub fn new(prefix: &str) -> Self {
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        let pid = std::process::id();
        let base_temp = std::env::temp_dir();
        for _ in 0..1024 {
            let id = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = base_temp.join(format!("{prefix}_{pid}_{id}"));
            match builder.create(&dir) {
                Ok(()) => return Self { path: dir },
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("Failed to create unique temp dir {}: {err}", dir.display()),
            }
        }
        panic!("Exhausted attempts to create unique temp dir for prefix {prefix}");
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
