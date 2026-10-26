//! General-purpose components for files no specific component covers.

use crate::error::Result;
use crate::output::Output;
use crate::project::Component;
use crate::toml_patch::{self, KeyPath};
use std::path::PathBuf;
use toml_edit::{DocumentMut, Item, Value};

/// A file framer generates in full.
#[derive(Clone, Debug)]
pub struct TextFile {
    path: PathBuf,
    contents: String,
}

impl TextFile {
    pub fn new(path: impl Into<PathBuf>, contents: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            contents: contents.into(),
        }
    }
}

impl Component for TextFile {
    fn synthesize(&self, output: &mut Output) -> Result<()> {
        output.managed(&self.path, &self.contents)
    }
}

/// A TOML file framer generates in full, such as `clippy.toml`.
#[derive(Clone, Debug)]
pub struct TomlFile {
    path: PathBuf,
    items: Vec<(KeyPath, Item)>,
}

impl TomlFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            items: Vec::new(),
        }
    }

    pub fn set(self, key: impl Into<KeyPath>, value: impl Into<Value>) -> Self {
        self.set_item(key, Item::Value(value.into()))
    }

    /// Sets a key to any TOML item, such as a whole table.
    pub fn set_item(mut self, key: impl Into<KeyPath>, item: Item) -> Self {
        self.items.push((key.into(), item));
        self
    }
}

impl Component for TomlFile {
    fn synthesize(&self, output: &mut Output) -> Result<()> {
        let mut doc = DocumentMut::new();
        for (key, item) in &self.items {
            toml_patch::set(&self.path, &mut doc, key, item.clone())?;
        }
        output.managed(&self.path, doc.to_string())
    }
}

/// Individual keys framer keeps up to date in a TOML file the user otherwise
/// owns, such as the `[workspace]` settings in `release-plz.toml`.
#[derive(Clone, Debug)]
pub struct TomlKeys {
    path: PathBuf,
    items: Vec<(KeyPath, Item)>,
}

impl TomlKeys {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            items: Vec::new(),
        }
    }

    pub fn set(self, key: impl Into<KeyPath>, value: impl Into<Value>) -> Self {
        self.set_item(key, Item::Value(value.into()))
    }

    /// Owns a key with any TOML item, such as a whole table.
    pub fn set_item(mut self, key: impl Into<KeyPath>, item: Item) -> Self {
        self.items.push((key.into(), item));
        self
    }
}

impl Component for TomlKeys {
    fn synthesize(&self, output: &mut Output) -> Result<()> {
        for (key, item) in &self.items {
            output.toml(&self.path, key.clone(), item.clone())?;
        }
        Ok(())
    }
}

/// A starting point written once, such as `src/main.rs` or a README skeleton.
/// framer never touches it again.
#[derive(Clone, Debug)]
pub struct SampleFile {
    path: PathBuf,
    contents: String,
}

impl SampleFile {
    pub fn new(path: impl Into<PathBuf>, contents: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            contents: contents.into(),
        }
    }
}

impl Component for SampleFile {
    fn synthesize(&self, output: &mut Output) -> Result<()> {
        output.sample(&self.path, &self.contents)
    }
}

/// A marked block framer keeps up to date inside a file the user otherwise
/// owns, such as shared conventions in `AGENTS.md` or entries in `.gitignore`.
///
/// The block is wrapped in `framer:begin <id>` and `framer:end <id>` comments,
/// written in the file's comment syntax (see [`crate::CommentStyle`]).
#[derive(Clone, Debug)]
pub struct TextBlock {
    path: PathBuf,
    id: String,
    contents: String,
}

impl TextBlock {
    pub fn new(
        path: impl Into<PathBuf>,
        id: impl Into<String>,
        contents: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            id: id.into(),
            contents: contents.into(),
        }
    }
}

impl Component for TextBlock {
    fn synthesize(&self, output: &mut Output) -> Result<()> {
        output.block(&self.path, &self.id, &self.contents)
    }
}
