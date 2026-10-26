use crate::error::{FramerError, Result};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

/// The files framer reads and writes. Paths are relative to the repository root.
pub trait FileSystem {
    /// The file's contents, or `None` when it doesn't exist.
    fn read(&self, path: &Path) -> Result<Option<String>>;
    /// Writes the file, creating parent directories. Fails if the file is read-only.
    fn write(&mut self, path: &Path, contents: &str) -> Result<()>;
    /// Removes the file. Removing a missing file succeeds.
    fn remove(&mut self, path: &Path) -> Result<()>;
    fn is_readonly(&self, path: &Path) -> Result<bool>;
    fn set_readonly(&mut self, path: &Path, readonly: bool) -> Result<()>;
}

/// A repository on disk.
#[derive(Clone, Debug)]
pub struct DiskFs {
    root: PathBuf,
}

impl DiskFs {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn io_error(path: &Path, source: io::Error) -> FramerError {
        FramerError::Io {
            path: path.to_owned(),
            source,
        }
    }
}

impl FileSystem for DiskFs {
    fn read(&self, path: &Path) -> Result<Option<String>> {
        match fs::read_to_string(self.root.join(path)) {
            Ok(contents) => Ok(Some(contents)),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(Self::io_error(path, error)),
        }
    }

    fn write(&mut self, path: &Path, contents: &str) -> Result<()> {
        let full = self.root.join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).map_err(|error| Self::io_error(path, error))?;
        }
        fs::write(full, contents).map_err(|error| Self::io_error(path, error))
    }

    fn remove(&mut self, path: &Path) -> Result<()> {
        match fs::remove_file(self.root.join(path)) {
            Err(error) if error.kind() != ErrorKind::NotFound => Err(Self::io_error(path, error)),
            _ => Ok(()),
        }
    }

    fn is_readonly(&self, path: &Path) -> Result<bool> {
        let metadata =
            fs::metadata(self.root.join(path)).map_err(|error| Self::io_error(path, error))?;
        Ok(metadata.permissions().readonly())
    }

    fn set_readonly(&mut self, path: &Path, readonly: bool) -> Result<()> {
        let full = self.root.join(path);
        let mut permissions = fs::metadata(&full)
            .map_err(|error| Self::io_error(path, error))?
            .permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // Clear every write bit, but only give write access back to the owner.
            let mode = permissions.mode();
            permissions.set_mode(if readonly {
                mode & !0o222
            } else {
                mode | 0o200
            });
        }
        #[cfg(not(unix))]
        permissions.set_readonly(readonly);
        fs::set_permissions(full, permissions).map_err(|error| Self::io_error(path, error))
    }
}

/// An in-memory repository for tests. Like a real file system, it refuses to
/// overwrite read-only files.
#[derive(Clone, Debug, Default)]
pub struct MemoryFs {
    files: BTreeMap<PathBuf, MemoryFile>,
}

#[derive(Clone, Debug)]
struct MemoryFile {
    contents: String,
    readonly: bool,
}

impl MemoryFs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_file(mut self, path: impl Into<PathBuf>, contents: impl Into<String>) -> Self {
        self.files.insert(
            path.into(),
            MemoryFile {
                contents: contents.into(),
                readonly: false,
            },
        );
        self
    }

    pub fn get(&self, path: impl AsRef<Path>) -> Option<&str> {
        self.files
            .get(path.as_ref())
            .map(|file| file.contents.as_str())
    }

    pub fn paths(&self) -> impl Iterator<Item = &Path> {
        self.files.keys().map(PathBuf::as_path)
    }

    fn not_found(path: &Path) -> FramerError {
        FramerError::Io {
            path: path.to_owned(),
            source: io::Error::from(ErrorKind::NotFound),
        }
    }
}

impl FileSystem for MemoryFs {
    fn read(&self, path: &Path) -> Result<Option<String>> {
        Ok(self.get(path).map(str::to_owned))
    }

    fn write(&mut self, path: &Path, contents: &str) -> Result<()> {
        match self.files.get_mut(path) {
            Some(file) if file.readonly => Err(FramerError::Io {
                path: path.to_owned(),
                source: io::Error::from(ErrorKind::PermissionDenied),
            }),
            Some(file) => {
                contents.clone_into(&mut file.contents);
                Ok(())
            }
            None => {
                self.files.insert(
                    path.to_owned(),
                    MemoryFile {
                        contents: contents.to_owned(),
                        readonly: false,
                    },
                );
                Ok(())
            }
        }
    }

    fn remove(&mut self, path: &Path) -> Result<()> {
        self.files.remove(path);
        Ok(())
    }

    fn is_readonly(&self, path: &Path) -> Result<bool> {
        self.files
            .get(path)
            .map(|file| file.readonly)
            .ok_or_else(|| Self::not_found(path))
    }

    fn set_readonly(&mut self, path: &Path, readonly: bool) -> Result<()> {
        let file = self
            .files
            .get_mut(path)
            .ok_or_else(|| Self::not_found(path))?;
        file.readonly = readonly;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn exercise(fs: &mut dyn FileSystem) {
        let path = Path::new("nested/dir/file.txt");

        assert_eq!(fs.read(path).unwrap(), None);
        fs.write(path, "one").unwrap();
        assert_eq!(fs.read(path).unwrap().as_deref(), Some("one"));

        fs.set_readonly(path, true).unwrap();
        assert!(fs.is_readonly(path).unwrap());
        assert!(fs.write(path, "two").is_err());

        fs.set_readonly(path, false).unwrap();
        fs.write(path, "two").unwrap();
        assert_eq!(fs.read(path).unwrap().as_deref(), Some("two"));

        fs.remove(path).unwrap();
        fs.remove(path).unwrap();
        assert_eq!(fs.read(path).unwrap(), None);
    }

    #[test]
    fn disk_fs_behaves_like_a_file_system() {
        let dir = TempDir::new().unwrap();
        exercise(&mut DiskFs::new(dir.path()));
    }

    #[test]
    fn memory_fs_behaves_like_disk_fs() {
        exercise(&mut MemoryFs::new());
    }
}
