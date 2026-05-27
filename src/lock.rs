use anyhow::{Context, Result, bail};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const STALE_LOCK_SECS: u64 = 600;

#[derive(Debug)]
pub struct LockFile {
    path: PathBuf,
    _file: File,
}

#[derive(Debug, Clone)]
pub struct LockInfo {
    pub pid: Option<u32>,
    pub timestamp: Option<u64>,
}

impl LockInfo {
    pub fn age(&self) -> Option<Duration> {
        self.timestamp
            .map(|ts| Duration::from_secs(now_unix_secs().saturating_sub(ts)))
    }

    pub fn is_stale(&self) -> bool {
        self.age()
            .is_some_and(|age| age.as_secs() >= STALE_LOCK_SECS)
    }

    pub fn display_age(&self) -> String {
        match self.age() {
            Some(age) if age.as_secs() >= 3600 => {
                format!(
                    "{}h {}m ago",
                    age.as_secs() / 3600,
                    (age.as_secs() % 3600) / 60
                )
            }
            Some(age) if age.as_secs() >= 60 => {
                format!("{}m {}s ago", age.as_secs() / 60, age.as_secs() % 60)
            }
            Some(age) => format!("{}s ago", age.as_secs()),
            None => "unknown age".to_string(),
        }
    }
}

impl LockFile {
    pub fn acquire(dir: &Path, name: &str) -> Result<Self> {
        fs::create_dir_all(dir)
            .context(format!("failed to create lock directory {}", dir.display()))?;

        let path = dir.join(format!("{name}.lock"));
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                let info = read_lock_info(&path);
                match &info {
                    LockInfo {
                        pid: Some(pid),
                        timestamp: Some(_ts),
                        ..
                    } if info.is_stale() => {
                        bail!(
                            "lock already held by process {} ({} — may be stale after {}s) at: {}",
                            pid,
                            info.display_age(),
                            STALE_LOCK_SECS,
                            path.display()
                        );
                    }
                    LockInfo {
                        pid: Some(pid),
                        timestamp: Some(_ts),
                        ..
                    } => {
                        bail!(
                            "lock already held by process {} ({}) at: {}",
                            pid,
                            info.display_age(),
                            path.display()
                        );
                    }
                    _ => {
                        bail!("lock already held at: {}", path.display());
                    }
                }
            }
            Err(error) => {
                return Err(error).context(format!("failed to create lock {}", path.display()));
            }
        };

        let pid = std::process::id();
        let ts = now_unix_secs();
        writeln!(file, "pid={pid}").context("failed to write pid to lock file")?;
        writeln!(file, "timestamp={ts}").context("failed to write timestamp to lock file")?;

        Ok(Self { path, _file: file })
    }
}

impl Drop for LockFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub fn read_lock_info(path: &Path) -> LockInfo {
    let (pid, timestamp) = match File::open(path) {
        Ok(file) => {
            let reader = BufReader::new(file);
            let (mut pid, mut timestamp) = (None, None);
            for line in reader.lines().map_while(Result::ok) {
                if let Some(value) = line.strip_prefix("pid=") {
                    pid = value.parse::<u32>().ok();
                } else if let Some(value) = line.strip_prefix("timestamp=") {
                    timestamp = value.parse::<u64>().ok();
                }
            }
            (pid, timestamp)
        }
        Err(_) => (None, None),
    };

    LockInfo { pid, timestamp }
}

pub fn app_lock_name(app: &str) -> String {
    format!("app-{app}")
}

pub fn volume_lock_name(app: &str) -> String {
    format!("volume-{app}")
}

pub fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn lock_rejects_concurrent_acquire() {
        let dir = temp_lock_dir("rejects-concurrent");
        let name = "app-test";

        let lock = LockFile::acquire(&dir, name).expect("acquire lock");
        let second = LockFile::acquire(&dir, name);

        assert!(second.is_err());
        assert!(
            second
                .unwrap_err()
                .to_string()
                .contains("lock already held")
        );

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

    #[test]
    fn lock_file_contains_pid_and_timestamp() {
        let dir = temp_lock_dir("contains-meta");
        let lock = LockFile::acquire(&dir, "app-test").expect("acquire lock");

        let info = read_lock_info(&lock.path);
        assert_eq!(info.pid, Some(std::process::id()));
        assert!(info.timestamp.is_some());

        drop(lock);
        fs::remove_dir_all(dir).expect("remove temp dir");
    }

    #[test]
    fn read_lock_info_from_empty_file() {
        let dir = temp_lock_dir("empty-file");
        let path = dir.join("empty.lock");
        fs::create_dir_all(&dir).expect("create dir");
        fs::write(&path, "").expect("write empty file");

        let info = read_lock_info(&path);
        assert!(info.pid.is_none());
        assert!(info.timestamp.is_none());

        fs::remove_file(&path).expect("remove file");
        fs::remove_dir_all(dir).expect("remove temp dir");
    }

    #[test]
    fn lock_info_displays_age_correctly() {
        let info = LockInfo {
            pid: Some(42),
            timestamp: Some(now_unix_secs() - 45),
        };
        assert!(info.display_age().contains("s ago"));

        let info = LockInfo {
            pid: Some(42),
            timestamp: Some(now_unix_secs() - 125),
        };
        assert!(info.display_age().contains("m "));

        let info = LockInfo {
            pid: Some(42),
            timestamp: Some(now_unix_secs() - 3720),
        };
        assert!(info.display_age().contains("h "));

        let info = LockInfo {
            pid: None,
            timestamp: None,
        };
        assert_eq!(info.display_age(), "unknown age");
    }

    #[test]
    fn stale_lock_is_detected() {
        let info = LockInfo {
            pid: Some(99),
            timestamp: Some(now_unix_secs() - STALE_LOCK_SECS - 1),
        };
        assert!(info.is_stale());
    }

    #[test]
    fn fresh_lock_is_not_stale() {
        let info = LockInfo {
            pid: Some(99),
            timestamp: Some(now_unix_secs() - 60),
        };
        assert!(!info.is_stale());
    }

    #[test]
    fn conflict_message_includes_pid_for_stale_lock() {
        let dir = temp_lock_dir("stale-msg");
        let path = dir.join("app-test.lock");
        fs::create_dir_all(&dir).expect("create dir");
        fs::write(
            &path,
            format!(
                "pid=777\ntimestamp={}\n",
                now_unix_secs() - STALE_LOCK_SECS - 30
            ),
        )
        .expect("write old lock");

        let result = LockFile::acquire(&dir, "app-test");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("777"));
        assert!(msg.contains("may be stale"));

        fs::remove_file(&path).expect("remove file");
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
