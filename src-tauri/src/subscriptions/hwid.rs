use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use uuid::{Uuid, Version};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct HwidStore {
    path: PathBuf,
}

impl HwidStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn get_or_create(&self) -> AppResult<Uuid> {
        match fs::read_to_string(&self.path) {
            Ok(value) => parse_stored_uuid(&value),
            Err(error) if error.kind() == ErrorKind::NotFound => {
                let uuid = Uuid::new_v4();
                self.write_atomic(uuid)?;
                Ok(uuid)
            }
            Err(error) => Err(error.into()),
        }
    }

    pub fn reset(&self) -> AppResult<Uuid> {
        let uuid = Uuid::new_v4();
        self.write_atomic(uuid)?;
        Ok(uuid)
    }

    #[cfg(test)]
    fn reset_with_replace<F>(&self, replace: F) -> AppResult<Uuid>
    where
        F: FnOnce(&Path, &Path) -> std::io::Result<()>,
    {
        let uuid = Uuid::new_v4();
        self.write_atomic_with(uuid, replace)?;
        Ok(uuid)
    }

    fn write_atomic(&self, uuid: Uuid) -> AppResult<()> {
        self.write_atomic_with(uuid, |temporary_path, target_path| {
            fs::rename(temporary_path, target_path)
        })
    }

    fn write_atomic_with<F>(&self, uuid: Uuid, replace: F) -> AppResult<()>
    where
        F: FnOnce(&Path, &Path) -> std::io::Result<()>,
    {
        ensure_parent(&self.path)?;
        let temporary_path = temporary_path(&self.path)?;
        let result = (|| -> AppResult<()> {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary_path)?;
            file.write_all(uuid.to_string().as_bytes())?;
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

fn parse_stored_uuid(value: &str) -> AppResult<Uuid> {
    let uuid = Uuid::parse_str(value.trim())
        .map_err(|_| AppError::Validation("stored HWID is not a valid UUID".into()))?;
    if uuid.get_version() != Some(Version::Random) {
        return Err(AppError::Validation(
            "stored HWID is not a random UUID v4".into(),
        ));
    }
    Ok(uuid)
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
        .ok_or_else(|| AppError::Validation("HWID path has no file name".into()))?
        .to_string_lossy();
    Ok(path.with_file_name(format!("{file_name}.{}.tmp", Uuid::new_v4())))
}

#[cfg(test)]
mod tests {
    use super::HwidStore;
    use std::fs;
    use std::io::{self, ErrorKind};
    use uuid::Uuid;

    #[test]
    fn get_or_create_is_stable_and_reset_rotates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("device-id");
        let store = HwidStore::new(path.clone());
        let first = store.get_or_create().unwrap();
        let second = store.get_or_create().unwrap();
        let third = store.reset().unwrap();

        assert_eq!(first, second);
        assert_ne!(first, third);
        assert_eq!(third.get_version_num(), 4);
        assert_eq!(fs::read_to_string(path).unwrap(), third.to_string());
    }

    #[test]
    fn failed_reset_preserves_previous_hwid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("device-id");
        let store = HwidStore::new(path.clone());
        let previous = store.get_or_create().unwrap();
        let previous_bytes = fs::read(&path).unwrap();

        let result = store.reset_with_replace(|_, _| {
            Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "forced replacement failure",
            ))
        });

        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), previous_bytes);
        assert_eq!(store.get_or_create().unwrap(), previous);
    }

    #[test]
    fn rejects_invalid_or_non_v4_persisted_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("device-id");
        let store = HwidStore::new(path.clone());

        fs::write(&path, "machine-name-derived-value").unwrap();
        assert!(store.get_or_create().is_err());

        fs::write(&path, Uuid::nil().to_string()).unwrap();
        assert!(store.get_or_create().is_err());
    }
}
