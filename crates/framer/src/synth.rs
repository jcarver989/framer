//! Turns a project's output into file changes: works out what each file should
//! contain, compares it with what's there, and either writes or reports the difference.

use crate::blocks;
use crate::error::{FramerError, Result};
use crate::format::{CommentStyle, add_header, strip_header};
use crate::fs::FileSystem;
use crate::manifest::{Entry, MANIFEST_PATH, Manifest};
use crate::output::{FileOutput, Output};
use crate::toml_patch::{self, KeyPath, TomlPatch};
use similar::TextDiff;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use toml_edit::{Item, Value};

/// Whether a run writes changes or only reports them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Write,
    /// Change nothing; report what a [`Mode::Write`] run would change.
    Check,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChangeKind {
    Created,
    Updated,
    Deleted,
    /// No longer generated, but kept as an ordinary file the user owns.
    Released,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileChange {
    pub path: PathBuf,
    pub kind: ChangeKind,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// The files a run changed, or in [`Mode::Check`], would change.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Report {
    pub changes: Vec<FileChange>,
}

impl Report {
    pub fn is_clean(&self) -> bool {
        self.changes.is_empty()
    }

    /// A unified diff of every change.
    pub fn diff(&self) -> String {
        let mut diff = String::new();
        for change in &self.changes {
            let path = change.path.display().to_string();
            let before = change.before.as_deref().unwrap_or_default();
            let after = change.after.as_deref().unwrap_or_default();
            let text = TextDiff::from_lines(before, after)
                .unified_diff()
                .header(&format!("a/{path}"), &format!("b/{path}"))
                .to_string();
            diff.push_str(&text);
        }
        diff
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.changes.is_empty() {
            return writeln!(f, "No changes.");
        }
        for change in &self.changes {
            let (symbol, label) = match change.kind {
                ChangeKind::Created => ('+', "created"),
                ChangeKind::Updated => ('~', "updated"),
                ChangeKind::Deleted => ('-', "deleted"),
                ChangeKind::Released => ('!', "released: no longer generated, now yours to edit"),
            };
            writeln!(f, "  {symbol} {} ({label})", change.path.display())?;
        }
        Ok(())
    }
}

/// What a single file should become.
struct Planned {
    path: PathBuf,
    before: Option<String>,
    after: Option<String>,
    readonly: bool,
    released: bool,
}

struct Planner<'a> {
    fs: &'a dyn FileSystem,
    old: &'a Manifest,
    new: Manifest,
    planned: Vec<Planned>,
}

fn manifest_key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

impl Planner<'_> {
    fn old_entry(&self, path: &Path) -> Option<&Entry> {
        self.old.files.get(&manifest_key(path))
    }

    fn plan(&mut self, path: &Path, before: Option<String>, after: Option<String>, readonly: bool) {
        self.planned.push(Planned {
            path: path.to_owned(),
            before,
            after,
            readonly,
            released: false,
        });
    }

    fn managed(
        &mut self,
        path: &Path,
        contents: &str,
        overrides: &[(KeyPath, Value)],
    ) -> Result<()> {
        let mut contents = contents.to_owned();
        if !overrides.is_empty() {
            let mut doc = toml_patch::parse(path, &contents)?;
            for (key, value) in overrides {
                toml_patch::set(path, &mut doc, key, Item::Value(value.clone()))?;
            }
            contents = doc.to_string();
        }
        let before = self.fs.read(path)?;
        self.new.files.insert(manifest_key(path), Entry::Managed);
        self.plan(path, before, Some(add_header(path, &contents)), true);
        Ok(())
    }

    fn toml(
        &mut self,
        path: &Path,
        owned: &TomlPatch,
        overrides: &[(KeyPath, Value)],
    ) -> Result<()> {
        let before = self.fs.read(path)?;
        let base = match (&before, self.old_entry(path)) {
            (Some(text), Some(Entry::Managed)) => strip_header(path, text),
            (Some(text), _) => text.clone(),
            (None, _) => String::new(),
        };
        let mut doc = toml_patch::parse(path, &base)?;

        let mut keys: Vec<KeyPath> = owned.keys().cloned().collect();
        for (key, _) in overrides {
            if !keys.iter().any(|owned| owned.contains(key)) {
                keys.push(key.clone());
            }
        }
        if let Some(Entry::Toml { keys: old_keys }) = self.old_entry(path) {
            for old_key in old_keys.iter().map(|key| KeyPath::from(key.as_str())) {
                // A table that still holds generated keys may also hold the user's.
                if !keys.iter().any(|key| old_key.contains(key)) {
                    toml_patch::remove(&mut doc, &old_key);
                }
            }
        }
        for (key, item) in owned.items() {
            toml_patch::set(path, &mut doc, key, item.clone())?;
        }
        for (key, value) in overrides {
            toml_patch::set(path, &mut doc, key, Item::Value(value.clone()))?;
        }

        let keys = keys.iter().map(ToString::to_string).collect();
        self.new
            .files
            .insert(manifest_key(path), Entry::Toml { keys });
        self.plan(path, before, Some(doc.to_string()), false);
        Ok(())
    }

    fn blocks(&mut self, path: &Path, blocks: &BTreeMap<String, String>) -> Result<()> {
        let style = CommentStyle::for_path(path);
        if style == CommentStyle::None {
            return Err(FramerError::Invalid {
                path: path.to_owned(),
                message: "cannot generate blocks in a file format without comments".into(),
            });
        }
        let before = self.fs.read(path)?;
        let mut text = before.clone().unwrap_or_default();
        if let Some(Entry::Blocks { ids }) = self.old_entry(path) {
            for id in ids.iter().filter(|id| !blocks.contains_key(*id)) {
                text = blocks::remove(&text, style, id, false);
            }
        }
        for (id, body) in blocks {
            text =
                blocks::upsert(&text, style, id, body).expect("the comment style supports blocks");
        }
        let ids = blocks.keys().cloned().collect();
        self.new
            .files
            .insert(manifest_key(path), Entry::Blocks { ids });
        self.plan(path, before, Some(text), false);
        Ok(())
    }

    fn sample(&mut self, path: &Path, contents: &str) -> Result<()> {
        if self.fs.read(path)?.is_some() {
            // Already the user's, but a file framer generated before still needs releasing.
            return self.release(path);
        }
        self.plan(path, None, Some(contents.to_owned()), false);
        Ok(())
    }

    /// Stops generating a file but keeps it as an ordinary file.
    fn release(&mut self, path: &Path) -> Result<()> {
        let Some(entry) = self.old_entry(path).cloned() else {
            return Ok(());
        };
        let Some(before) = self.fs.read(path)? else {
            return Ok(());
        };
        let after = match entry {
            Entry::Managed => strip_header(path, &before),
            Entry::Blocks { ids } => {
                let style = CommentStyle::for_path(path);
                ids.iter().fold(before.clone(), |text, id| {
                    blocks::remove(&text, style, id, true)
                })
            }
            Entry::Toml { .. } => before.clone(),
        };
        self.planned.push(Planned {
            path: path.to_owned(),
            before: Some(before),
            after: Some(after),
            readonly: false,
            released: true,
        });
        Ok(())
    }

    /// Undoes what a previous run generated in a file that's no longer generated.
    fn retire(&mut self, path: &Path) -> Result<()> {
        let Some(entry) = self.old_entry(path).cloned() else {
            return Ok(());
        };
        let Some(before) = self.fs.read(path)? else {
            return Ok(());
        };
        let after = match entry {
            Entry::Managed => String::new(),
            Entry::Blocks { ids } => {
                let style = CommentStyle::for_path(path);
                ids.iter().fold(before.clone(), |text, id| {
                    blocks::remove(&text, style, id, false)
                })
            }
            Entry::Toml { keys } => {
                let mut doc = toml_patch::parse(path, &before)?;
                for key in &keys {
                    toml_patch::remove(&mut doc, &KeyPath::from(key.as_str()));
                }
                doc.to_string()
            }
        };
        let after = (!after.trim().is_empty()).then_some(after);
        self.plan(path, Some(before), after, false);
        Ok(())
    }
}

pub(crate) fn run(
    fs: &mut dyn FileSystem,
    output: Output,
    overrides: &BTreeMap<PathBuf, Vec<(KeyPath, Value)>>,
    unmanaged: &BTreeSet<PathBuf>,
    mode: Mode,
) -> Result<Report> {
    let mut files = output.into_files();
    for path in overrides.keys() {
        files
            .entry(path.clone())
            .or_insert_with(|| FileOutput::Toml(TomlPatch::default()));
    }
    let old = Manifest::load(fs)?;

    let mut paths: BTreeSet<PathBuf> = files.keys().cloned().collect();
    paths.extend(old.files.keys().map(PathBuf::from));

    let mut planner = Planner {
        fs: &*fs,
        old: &old,
        new: Manifest::default(),
        planned: Vec::new(),
    };
    for path in &paths {
        let file_overrides = overrides.get(path).map(Vec::as_slice).unwrap_or_default();
        if unmanaged.contains(path) {
            planner.release(path)?;
            continue;
        }
        match files.get(path) {
            Some(FileOutput::Managed(contents)) => {
                planner.managed(path, contents, file_overrides)?;
            }
            Some(FileOutput::Toml(patch)) => planner.toml(path, patch, file_overrides)?,
            Some(FileOutput::Blocks(blocks)) => planner.blocks(path, blocks)?,
            Some(FileOutput::Sample(contents)) => planner.sample(path, contents)?,
            None => planner.retire(path)?,
        }
    }

    let new_manifest = planner.new;
    let mut plans = planner.planned;
    let manifest_path = Path::new(MANIFEST_PATH);
    let before = fs.read(manifest_path)?;
    if before.is_some() || !new_manifest.files.is_empty() {
        plans.push(Planned {
            path: manifest_path.to_owned(),
            before,
            after: Some(new_manifest.to_json()),
            readonly: false,
            released: false,
        });
    }

    let report = Report {
        changes: plans.iter().filter_map(change).collect(),
    };
    if mode == Mode::Write {
        apply(fs, &plans)?;
    }
    Ok(report)
}

fn change(planned: &Planned) -> Option<FileChange> {
    if planned.before == planned.after {
        return None;
    }
    let kind = match (&planned.before, &planned.after) {
        _ if planned.released => ChangeKind::Released,
        (None, _) => ChangeKind::Created,
        (_, None) => ChangeKind::Deleted,
        _ => ChangeKind::Updated,
    };
    Some(FileChange {
        path: planned.path.clone(),
        kind,
        before: planned.before.clone(),
        after: planned.after.clone(),
    })
}

fn apply(fs: &mut dyn FileSystem, planned: &[Planned]) -> Result<()> {
    for file in planned {
        let exists = file.before.is_some();
        let changed = file.before != file.after;
        if exists && (changed || !file.readonly) && fs.is_readonly(&file.path)? {
            fs.set_readonly(&file.path, false)?;
        }
        match &file.after {
            None => fs.remove(&file.path)?,
            Some(after) if changed => fs.write(&file.path, after)?,
            Some(_) => {}
        }
        if file.readonly && file.after.is_some() {
            fs.set_readonly(&file.path, true)?;
        }
    }
    Ok(())
}

/// Stops using framer: every generated file becomes an ordinary file, and the
/// manifest is removed. The `.framer/` project itself is left for the user to delete.
pub fn eject(fs: &mut dyn FileSystem) -> Result<Report> {
    let old = Manifest::load(fs)?;
    let mut planner = Planner {
        fs: &*fs,
        old: &old,
        new: Manifest::default(),
        planned: Vec::new(),
    };
    for path in old.files.keys() {
        planner.release(Path::new(path))?;
    }
    let mut plans = planner.planned;
    let manifest_path = Path::new(MANIFEST_PATH);
    if let Some(before) = fs.read(manifest_path)? {
        plans.push(Planned {
            path: manifest_path.to_owned(),
            before: Some(before),
            after: None,
            readonly: false,
            released: false,
        });
    }
    let report = Report {
        changes: plans.iter().filter_map(change).collect(),
    };
    apply(fs, &plans)?;
    Ok(report)
}
