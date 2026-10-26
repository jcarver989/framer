use crate::error::{FramerError, Result};
use crate::toml_patch::{KeyPath, TomlPatch};
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::path::PathBuf;
use toml_edit::Item;

/// What the components of a project generate. Each file has exactly one kind
/// of output, and every file, TOML key and block has exactly one owner.
#[derive(Debug, Default)]
pub struct Output {
    files: BTreeMap<PathBuf, FileOutput>,
}

#[derive(Debug)]
pub(crate) enum FileOutput {
    /// framer owns the whole file.
    Managed(String),
    /// framer owns some keys of a TOML file.
    Toml(TomlPatch),
    /// framer owns some marked blocks of a text file, by block id.
    Blocks(BTreeMap<String, String>),
    /// Written once if missing, then left to the user.
    Sample(String),
}

impl FileOutput {
    fn describe(&self) -> &'static str {
        match self {
            Self::Managed(_) => "a generated file",
            Self::Toml(_) => "a TOML file with generated keys",
            Self::Blocks(_) => "a file with generated blocks",
            Self::Sample(_) => "a sample file",
        }
    }
}

impl Output {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn conflict(path: PathBuf, message: String) -> FramerError {
        FramerError::Conflict { path, message }
    }

    /// Generates the whole file. framer adds a "do not edit" header and makes it read-only.
    pub fn managed(&mut self, path: impl Into<PathBuf>, contents: impl Into<String>) -> Result<()> {
        match self.files.entry(path.into()) {
            Entry::Vacant(entry) => {
                entry.insert(FileOutput::Managed(contents.into()));
                Ok(())
            }
            Entry::Occupied(entry) => {
                let message = format!("already generated as {}", entry.get().describe());
                Err(Self::conflict(entry.key().clone(), message))
            }
        }
    }

    /// Generates one key of a TOML file, leaving the rest of the file to the user.
    pub fn toml(
        &mut self,
        path: impl Into<PathBuf>,
        key: impl Into<KeyPath>,
        item: Item,
    ) -> Result<()> {
        let path = path.into();
        let key = key.into();
        let output = self
            .files
            .entry(path.clone())
            .or_insert_with(|| FileOutput::Toml(TomlPatch::default()));
        let FileOutput::Toml(owned) = output else {
            let message = format!(
                "cannot generate `{key}`: already generated as {}",
                output.describe()
            );
            return Err(Self::conflict(path, message));
        };
        owned.own(key.clone(), item).map_err(|existing| {
            Self::conflict(
                path,
                format!("`{key}` overlaps `{existing}`, which is already generated"),
            )
        })
    }

    /// Generates the block `id` of a text file, leaving the rest of the file to the user.
    pub fn block(
        &mut self,
        path: impl Into<PathBuf>,
        id: impl Into<String>,
        contents: impl Into<String>,
    ) -> Result<()> {
        let path = path.into();
        let id = id.into();
        let output = self
            .files
            .entry(path.clone())
            .or_insert_with(|| FileOutput::Blocks(BTreeMap::new()));
        let FileOutput::Blocks(blocks) = output else {
            let message = format!(
                "cannot generate block `{id}`: already generated as {}",
                output.describe()
            );
            return Err(Self::conflict(path, message));
        };
        match blocks.entry(id) {
            Entry::Vacant(entry) => {
                entry.insert(contents.into());
                Ok(())
            }
            Entry::Occupied(entry) => {
                let message = format!("block `{}` is already generated", entry.key());
                Err(Self::conflict(path, message))
            }
        }
    }

    /// Writes the file once if it's missing. After that it belongs to the user.
    pub fn sample(&mut self, path: impl Into<PathBuf>, contents: impl Into<String>) -> Result<()> {
        match self.files.entry(path.into()) {
            Entry::Vacant(entry) => {
                entry.insert(FileOutput::Sample(contents.into()));
                Ok(())
            }
            Entry::Occupied(entry) => {
                let message = format!("already generated as {}", entry.get().describe());
                Err(Self::conflict(entry.key().clone(), message))
            }
        }
    }

    pub(crate) fn into_files(self) -> BTreeMap<PathBuf, FileOutput> {
        self.files
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toml_edit::value;

    #[test]
    fn a_file_has_one_kind_of_output() {
        let mut output = Output::new();
        output.managed("ci.yml", "a").unwrap();

        let error = output.toml("ci.yml", "a.b", value(1)).unwrap_err();

        assert_eq!(
            error.to_string(),
            "ci.yml: cannot generate `a.b`: already generated as a generated file"
        );
        assert!(output.managed("ci.yml", "b").is_err());
        assert!(output.sample("ci.yml", "b").is_err());
    }

    #[test]
    fn toml_keys_and_blocks_have_one_owner() {
        let mut output = Output::new();
        output
            .toml("Cargo.toml", "workspace.package", value(1))
            .unwrap();
        output.toml("Cargo.toml", "profile.dist", value(1)).unwrap();
        output.block("AGENTS.md", "a", "x").unwrap();
        output.block("AGENTS.md", "b", "x").unwrap();

        assert!(output.toml("Cargo.toml", "workspace", value(1)).is_err());
        assert!(output.block("AGENTS.md", "a", "y").is_err());
    }
}
