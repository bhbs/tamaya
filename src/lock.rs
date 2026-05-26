use anyhow::{Context, Result, bail};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct LockFile {
    path: PathBuf,
    _file: File,
}

impl LockFile {
    pub fn acquire(dir: &Path, name: &str) -> Result<Self> {
        fs::create_dir_all(dir)
            .context(format!("failed to create lock directory {}", dir.display()))?;

        let path = dir.join(format!("{name}.lock"));
        let file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                bail!("lock already held: {}", path.display())
            }
            Err(error) => {
                return Err(error).context(format!("failed to create lock {}", path.display()));
            }
        };

        Ok(Self { path, _file: file })
    }
}

impl Drop for LockFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub fn app_lock_name(app: &str) -> String {
    format!("app-{app}")
}

pub fn volume_lock_name(app: &str) -> String {
    format!("volume-{app}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn lock_rejects_concurrent_acquire() {
        let dir = std::env::temp_dir().join(format!(
            "v-lock-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));

        let lock = LockFile::acquire(&dir, "app-test").expect("acquire lock");
        let second = LockFile::acquire(&dir, "app-test");

        assert!(second.is_err());

        drop(lock);
        fs::remove_dir_all(dir).expect("remove temp dir");
    }

    #[test]
    fn lock_file_is_removed_on_drop() {
        let dir = temp_lock_dir("drop");
        let path = dir.join("app-test.lock");

        {
            let _lock = LockFile::acquire(&dir, "app-test").expect("acquire lock");
            assert!(path.exists());
        }

        assert!(!path.exists());

        fs::remove_dir_all(dir).expect("remove temp dir");
    }

    #[test]
    fn lock_names_are_scoped() {
        assert_eq!(app_lock_name("web"), "app-web");
        assert_eq!(volume_lock_name("web"), "volume-web");
    }

    #[test]
    fn lock_reports_create_file_errors() {
        let dir = temp_lock_dir("nested-missing-parent");
        let result = LockFile::acquire(&dir, "missing/child");

        assert!(result.is_err());

        fs::remove_dir_all(dir).expect("remove temp dir");
    }

    fn temp_lock_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "v-lock-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ))
    }
}
