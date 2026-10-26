//! Owning individual keys of a TOML file while leaving the rest of it, and its
//! formatting, to the user.

use crate::error::{FramerError, Result};
use std::fmt;
use std::path::Path;
use toml_edit::{DocumentMut, InlineTable, Item, RawString, Table, Value};

/// A dotted path to a TOML key, such as `workspace.package` or `profile.dist.lto`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct KeyPath(Vec<String>);

impl KeyPath {
    /// A path from segments, for keys that themselves contain dots.
    pub fn new<T: Into<String>>(segments: impl IntoIterator<Item = T>) -> Self {
        Self(segments.into_iter().map(Into::into).collect())
    }

    pub fn segments(&self) -> &[String] {
        &self.0
    }

    /// Whether `self` is `other` or one of its parents.
    pub fn contains(&self, other: &Self) -> bool {
        other.0.starts_with(&self.0)
    }

    pub(crate) fn overlaps(&self, other: &Self) -> bool {
        self.contains(other) || other.contains(self)
    }
}

impl From<&str> for KeyPath {
    fn from(key: &str) -> Self {
        Self::new(key.split('.'))
    }
}

impl From<String> for KeyPath {
    fn from(key: String) -> Self {
        Self::from(key.as_str())
    }
}

impl fmt::Display for KeyPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.join("."))
    }
}

/// The keys one run of framer owns in a TOML file.
#[derive(Clone, Debug, Default)]
pub(crate) struct TomlPatch {
    items: Vec<(KeyPath, Item)>,
}

impl TomlPatch {
    /// Claims `key`. Fails with the already-claimed key it overlaps, if any.
    pub(crate) fn own(&mut self, key: KeyPath, item: Item) -> Result<(), KeyPath> {
        if let Some((existing, _)) = self.items.iter().find(|(owned, _)| owned.overlaps(&key)) {
            return Err(existing.clone());
        }
        self.items.push((key, item));
        Ok(())
    }

    pub(crate) fn keys(&self) -> impl Iterator<Item = &KeyPath> {
        self.items.iter().map(|(key, _)| key)
    }

    pub(crate) fn items(&self) -> &[(KeyPath, Item)] {
        &self.items
    }
}

pub(crate) fn parse(path: &Path, text: &str) -> Result<DocumentMut> {
    text.parse().map_err(|source| FramerError::Toml {
        path: path.to_owned(),
        source,
    })
}

fn implicit_table() -> Item {
    let mut table = Table::new();
    table.set_implicit(true);
    Item::Table(table)
}

/// The child table at `key`, created if missing. `None` when `item` isn't a table.
fn child<'a>(item: &'a mut Item, key: &str) -> Option<&'a mut Item> {
    let fresh = match item {
        Item::None => {
            *item = implicit_table();
            implicit_table()
        }
        Item::Table(_) => implicit_table(),
        Item::Value(Value::InlineTable(_)) => Item::Value(Value::InlineTable(InlineTable::new())),
        _ => return None,
    };
    let table = item.as_table_like_mut()?;
    if !table.contains_key(key) {
        table.insert(key, fresh);
    }
    table.get_mut(key)
}

/// Keeps a replaced item where it was in the file, with its surrounding
/// whitespace and comments, so re-running framer doesn't move things around.
fn preserve_format(old: &Item, new: &mut Item) {
    match (old, new) {
        (Item::Table(old), Item::Table(new)) => {
            new.set_position(old.position());
            *new.decor_mut() = old.decor().clone();
        }
        (Item::Value(old), Item::Value(new)) => *new.decor_mut() = old.decor().clone(),
        _ => {}
    }
}

/// Sets `key` to `item`, replacing whatever was there.
pub(crate) fn set(path: &Path, doc: &mut DocumentMut, key: &KeyPath, item: Item) -> Result<()> {
    let invalid = |message: String| FramerError::Invalid {
        path: path.to_owned(),
        message,
    };
    let Some((leaf, parents)) = key.segments().split_last() else {
        return Err(invalid("cannot set an empty TOML key".into()));
    };

    let mut current = doc.as_item_mut();
    for (depth, segment) in parents.iter().enumerate() {
        current = child(current, segment).ok_or_else(|| {
            let parent = KeyPath::new(&parents[..depth]);
            invalid(format!("cannot set `{key}`: `{parent}` is not a table"))
        })?;
    }
    if !parents.is_empty() && !item.is_table() {
        separate_newly_visible_header(current);
    }

    let inline_parent = matches!(current, Item::Value(_));
    let table = current.as_table_like_mut().ok_or_else(|| {
        let parent = KeyPath::new(parents);
        invalid(format!("cannot set `{key}`: `{parent}` is not a table"))
    })?;
    let mut item = if inline_parent {
        item.into_value().map_or_else(|item| item, Item::Value)
    } else {
        item
    };
    if let Some(existing) = table.get(leaf) {
        preserve_format(existing, &mut item);
    }
    table.insert(leaf, item);
    Ok(())
}

/// An implicit table, such as `[workspace]` in a file that only has
/// `[workspace.package]`, gets a header once it holds a value. Its child tables
/// then need a blank line before them, which the file's first table never had.
fn separate_newly_visible_header(item: &mut Item) {
    let Item::Table(table) = item else {
        return;
    };
    if !table.is_implicit() {
        return;
    }
    table.set_implicit(false);
    for (_, child) in table.iter_mut() {
        if let Item::Table(child) = child {
            let prefix = child.decor().prefix().and_then(RawString::as_str);
            if prefix == Some("") {
                child.decor_mut().set_prefix("\n");
            }
        }
    }
}

/// Removes `key` if it's there, along with any tables that removing it leaves empty.
pub(crate) fn remove(doc: &mut DocumentMut, key: &KeyPath) {
    remove_within(doc.as_item_mut(), key.segments());
}

/// Returns whether `item` is an empty table afterwards.
fn remove_within(item: &mut Item, path: &[String]) -> bool {
    let Some(table) = item.as_table_like_mut() else {
        return false;
    };
    match path {
        [] => return false,
        [leaf] => {
            table.remove(leaf);
        }
        [first, rest @ ..] => {
            let Some(child) = table.get_mut(first) else {
                return false;
            };
            if remove_within(child, rest) {
                table.remove(first);
            }
        }
    }
    table.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;
    use toml_edit::{Array, value};

    const PATH: &str = "Cargo.toml";

    fn patched(text: &str, items: &[(&str, Item)]) -> String {
        let mut doc = parse(Path::new(PATH), text).unwrap();
        for (key, item) in items {
            set(
                Path::new(PATH),
                &mut doc,
                &KeyPath::from(*key),
                item.clone(),
            )
            .unwrap();
        }
        doc.to_string()
    }

    fn package() -> Item {
        let mut table = Table::new();
        table.insert("edition", value("2024"));
        table.insert("license", value("MIT"));
        Item::Table(table)
    }

    fn profile() -> Item {
        let mut table = Table::new();
        table.insert("inherits", value("release"));
        Item::Table(table)
    }

    #[test]
    fn replaces_owned_tables_and_keeps_everything_else() {
        let text = indoc! {r#"
            # Workspace settings
            [workspace]
            members = ["crates/*"] # every crate

            [workspace.package]
            edition = "2021"

            [workspace.dependencies]
            serde = "1"
        "#};

        let result = patched(
            text,
            &[
                (
                    "workspace.members",
                    value(Array::from_iter(["crates/*", "xtask"])),
                ),
                ("workspace.package", package()),
            ],
        );

        assert_eq!(
            result,
            indoc! {r#"
                # Workspace settings
                [workspace]
                members = ["crates/*", "xtask"] # every crate

                [workspace.package]
                edition = "2024"
                license = "MIT"

                [workspace.dependencies]
                serde = "1"
            "#}
        );
    }

    #[test]
    fn creates_nested_tables_without_empty_parent_headers() {
        let result = patched(
            "",
            &[
                ("workspace.package", package()),
                ("profile.dist", profile()),
            ],
        );

        assert_eq!(
            result,
            indoc! {r#"
                [workspace.package]
                edition = "2024"
                license = "MIT"

                [profile.dist]
                inherits = "release"
            "#}
        );
    }

    #[test]
    fn applying_the_same_patch_twice_changes_nothing() {
        let items = [
            ("workspace.resolver", value("3")),
            ("workspace.package", package()),
            ("profile.dist", profile()),
        ];
        let once = patched("[package]\nname = \"demo\"\n", &items);
        let twice = patched(&once, &items);

        assert_eq!(once, twice);
    }

    #[test]
    fn sets_keys_inside_inline_tables() {
        let result = patched(
            "[lints]\nclippy = { all = \"warn\" }\n",
            &[("lints.clippy.pedantic", value("warn"))],
        );

        assert_eq!(
            result,
            "[lints]\nclippy = { all = \"warn\" , pedantic = \"warn\" }\n"
        );
    }

    #[test]
    fn a_parent_table_that_gains_a_header_is_separated_from_its_children() {
        let text = "[workspace.package]\nversion = \"0.1.0\"\n";

        let result = patched(text, &[("workspace.resolver", value("3"))]);

        assert_eq!(
            result,
            "[workspace]\nresolver = \"3\"\n\n[workspace.package]\nversion = \"0.1.0\"\n"
        );
        assert_eq!(
            patched(&result, &[("workspace.resolver", value("3"))]),
            result
        );
    }

    #[test]
    fn refuses_to_descend_into_values() {
        let mut doc = parse(Path::new(PATH), "workspace = 1\n").unwrap();
        let error = set(
            Path::new(PATH),
            &mut doc,
            &"workspace.members".into(),
            value("x"),
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Cargo.toml: cannot set `workspace.members`: `workspace` is not a table"
        );
    }

    #[test]
    fn removes_keys() {
        let mut doc = parse(
            Path::new(PATH),
            "[profile.dist]\nlto = true\n[profile.dev]\nopt-level = 1\n",
        )
        .unwrap();
        remove(&mut doc, &"profile.dist".into());
        remove(&mut doc, &"missing.key".into());

        assert_eq!(doc.to_string(), "[profile.dev]\nopt-level = 1\n");
    }

    #[test]
    fn overlapping_keys_conflict() {
        let mut patch = TomlPatch::default();
        patch.own("profile.dist".into(), profile()).unwrap();

        assert_eq!(
            patch.own("profile".into(), profile()),
            Err("profile.dist".into())
        );
        assert_eq!(
            patch.own("profile.dist.lto".into(), value(true)),
            Err("profile.dist".into())
        );
        assert!(patch.own("profile.release".into(), profile()).is_ok());
    }
}
