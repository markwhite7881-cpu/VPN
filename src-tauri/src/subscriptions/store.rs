use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

use super::model::SubscriptionRecord;

#[derive(Debug, Clone)]
pub struct SubscriptionStore {
    path: PathBuf,
}

impl SubscriptionStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load_all(&self) -> AppResult<Vec<SubscriptionRecord>> {
        match fs::read(&self.path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn replace_all(&self, records: &[SubscriptionRecord]) -> AppResult<()> {
        self.replace_all_with(records, |temporary_path, target_path| {
            fs::rename(temporary_path, target_path)
        })
    }

    #[cfg(test)]
    fn replace_all_with_replace<F>(
        &self,
        records: &[SubscriptionRecord],
        replace: F,
    ) -> AppResult<()>
    where
        F: FnOnce(&Path, &Path) -> std::io::Result<()>,
    {
        self.replace_all_with(records, replace)
    }

    fn replace_all_with<F>(&self, records: &[SubscriptionRecord], replace: F) -> AppResult<()>
    where
        F: FnOnce(&Path, &Path) -> std::io::Result<()>,
    {
        ensure_parent(&self.path)?;
        let temporary_path = temporary_path(&self.path)?;
        let result = (|| -> AppResult<()> {
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&temporary_path)?;
            serde_json::to_writer_pretty(&mut file, records)?;
            file.write_all(b"\n")?;
            file.flush()?;
            file.sync_all()?;
            drop(file);
            replace(&temporary_path, &self.path)?;
            Ok(())
        })();

        if result.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }
        result
    }
}

fn ensure_parent(path: &Path) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn temporary_path(path: &Path) -> AppResult<PathBuf> {
    let file_name = path
        .file_name()
        .ok_or_else(|| AppError::Validation("subscription store path has no file name".into()))?
        .to_string_lossy();
    Ok(path.with_file_name(format!("{file_name}.tmp")))
}

#[cfg(test)]
mod tests {
    use super::SubscriptionStore;
    use crate::subscriptions::model::{ProviderMetadata, SubscriptionKind, SubscriptionRecord};
    use std::fs;
    use std::io::{self, ErrorKind};

    fn sample_record(name: &str) -> SubscriptionRecord {
        SubscriptionRecord {
            id: "sub-1".into(),
            name: name.into(),
            url: "https://example.test/private-token".into(),
            kind: SubscriptionKind::Auto,
            engine: None,
            interval_minutes: 60,
            active_child_key: None,
            children: Vec::new(),
            metadata: ProviderMetadata::default(),
            last_success_at: None,
            last_http_status: None,
            last_error: None,
        }
    }

    #[test]
    fn missing_store_loads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = SubscriptionStore::new(dir.path().join("subscriptions.json"));
        assert!(store.load_all().unwrap().is_empty());
    }

    #[test]
    fn failed_replace_preserves_previous_store() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("subscriptions.json");
        let store = SubscriptionStore::new(path.clone());
        store.replace_all(&[sample_record("First")]).unwrap();
        let previous_bytes = fs::read(&path).unwrap();

        let result = store.replace_all_with_replace(&[sample_record("Replacement")], |_, _| {
            Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "forced replacement failure",
            ))
        });

        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), previous_bytes);
        assert_eq!(store.load_all().unwrap()[0].name, "First");
    }

    #[test]
    fn replace_all_atomically_replaces_and_survives_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("subscriptions.json");
        let store = SubscriptionStore::new(path.clone());

        store.replace_all(&[sample_record("First")]).unwrap();
        store.replace_all(&[sample_record("Replacement")]).unwrap();

        let records = SubscriptionStore::new(path.clone()).load_all().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].name, "Replacement");
        assert!(!path.with_file_name("subscriptions.json.tmp").exists());
    }
}
