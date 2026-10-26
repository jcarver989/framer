//! Components for Rust workspaces: workspace settings in `Cargo.toml`, lints,
//! profiles and the pinned toolchain.

use crate::error::Result;
use crate::output::Output;
use crate::project::Component;
use crate::toml_patch::KeyPath;
use toml_edit::{Array, DocumentMut, InlineTable, Item, Table, Value, value};

const CARGO_TOML: &str = "Cargo.toml";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Edition {
    E2015,
    E2018,
    E2021,
    E2024,
}

impl Edition {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::E2015 => "2015",
            Self::E2018 => "2018",
            Self::E2021 => "2021",
            Self::E2024 => "2024",
        }
    }
}

fn array<T: Into<String>>(values: impl IntoIterator<Item = T>) -> Array {
    values
        .into_iter()
        .map(|value| Value::from(value.into()))
        .collect()
}

/// `rust-toolchain.toml`, which pins the toolchain for everyone, CI included.
#[derive(Clone, Debug)]
pub struct Toolchain {
    channel: String,
    components: Vec<String>,
    targets: Vec<String>,
    profile: Option<String>,
}

impl Toolchain {
    /// A toolchain pinned to an exact release, such as `1.98.0`.
    pub fn pinned(version: impl Into<String>) -> Self {
        Self {
            channel: version.into(),
            components: Vec::new(),
            targets: Vec::new(),
            profile: Some("minimal".into()),
        }
    }

    /// Whatever the latest stable release is.
    pub fn stable() -> Self {
        Self::pinned("stable")
    }

    pub fn components<T: Into<String>>(mut self, names: impl IntoIterator<Item = T>) -> Self {
        self.components.extend(names.into_iter().map(Into::into));
        self
    }

    pub fn targets<T: Into<String>>(mut self, triples: impl IntoIterator<Item = T>) -> Self {
        self.targets.extend(triples.into_iter().map(Into::into));
        self
    }

    /// The rustup profile, `minimal` by default. `None` uses rustup's default.
    pub fn profile(mut self, profile: Option<&str>) -> Self {
        self.profile = profile.map(Into::into);
        self
    }
}

impl Component for Toolchain {
    fn synthesize(&self, output: &mut Output) -> Result<()> {
        let mut toolchain = Table::new();
        toolchain.insert("channel", value(&self.channel));
        if !self.components.is_empty() {
            toolchain.insert("components", value(array(&self.components)));
        }
        if !self.targets.is_empty() {
            toolchain.insert("targets", value(array(&self.targets)));
        }
        if let Some(profile) = &self.profile {
            toolchain.insert("profile", value(profile));
        }
        let mut doc = DocumentMut::new();
        doc.insert("toolchain", Item::Table(toolchain));
        output.managed("rust-toolchain.toml", doc.to_string())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Level {
    Allow,
    Warn,
    Deny,
    Forbid,
}

impl Level {
    fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Warn => "warn",
            Self::Deny => "deny",
            Self::Forbid => "forbid",
        }
    }
}

#[derive(Clone, Debug)]
struct Lint {
    name: String,
    level: Level,
    priority: Option<i64>,
}

/// `[workspace.lints]`. Member crates opt in with `[lints] workspace = true`.
#[derive(Clone, Debug, Default)]
pub struct Lints {
    rust: Vec<Lint>,
    clippy: Vec<Lint>,
}

/// Lint groups get a lower priority so individual lints can override them.
const GROUP_PRIORITY: i64 = -1;

fn upsert(lints: &mut Vec<Lint>, lint: Lint) {
    match lints.iter_mut().find(|existing| existing.name == lint.name) {
        Some(existing) => *existing = lint,
        None => lints.push(lint),
    }
}

fn lints_table(lints: &[Lint]) -> Table {
    let mut table = Table::new();
    for lint in lints {
        let setting = match lint.priority {
            Some(priority) => {
                let mut inline = InlineTable::new();
                inline.insert("level", lint.level.as_str().into());
                inline.insert("priority", priority.into());
                Value::InlineTable(inline)
            }
            None => lint.level.as_str().into(),
        };
        table.insert(&lint.name, value(setting));
    }
    table
}

impl Lints {
    pub fn new() -> Self {
        Self::default()
    }

    /// Clippy's `all` and `pedantic` groups as warnings.
    pub fn clippy_pedantic() -> Self {
        Self::new()
            .clippy_group("all", Level::Warn)
            .clippy_group("pedantic", Level::Warn)
    }

    pub fn clippy(mut self, name: impl Into<String>, level: Level) -> Self {
        let lint = Lint {
            name: name.into(),
            level,
            priority: None,
        };
        upsert(&mut self.clippy, lint);
        self
    }

    pub fn clippy_group(mut self, name: impl Into<String>, level: Level) -> Self {
        let lint = Lint {
            name: name.into(),
            level,
            priority: Some(GROUP_PRIORITY),
        };
        upsert(&mut self.clippy, lint);
        self
    }

    pub fn allow_clippy<T: Into<String>>(self, names: impl IntoIterator<Item = T>) -> Self {
        names
            .into_iter()
            .fold(self, |lints, name| lints.clippy(name, Level::Allow))
    }

    pub fn rust(mut self, name: impl Into<String>, level: Level) -> Self {
        let lint = Lint {
            name: name.into(),
            level,
            priority: None,
        };
        upsert(&mut self.rust, lint);
        self
    }

    pub fn rust_group(mut self, name: impl Into<String>, level: Level) -> Self {
        let lint = Lint {
            name: name.into(),
            level,
            priority: Some(GROUP_PRIORITY),
        };
        upsert(&mut self.rust, lint);
        self
    }

    fn to_item(&self) -> Item {
        let mut table = Table::new();
        table.set_implicit(true);
        if !self.rust.is_empty() {
            table.insert("rust", Item::Table(lints_table(&self.rust)));
        }
        if !self.clippy.is_empty() {
            table.insert("clippy", Item::Table(lints_table(&self.clippy)));
        }
        Item::Table(table)
    }
}

/// A `[profile.<name>]` table.
#[derive(Clone, Debug)]
pub struct Profile {
    name: String,
    table: Table,
}

impl Profile {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            table: Table::new(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn inherits(self, profile: &str) -> Self {
        self.set("inherits", profile)
    }

    /// `"thin"`, `"fat"`, `true` or `false`.
    pub fn lto(self, lto: impl Into<Value>) -> Self {
        self.set("lto", lto)
    }

    pub fn codegen_units(self, units: i64) -> Self {
        self.set("codegen-units", units)
    }

    /// `0` to `3`, `"s"` or `"z"`.
    pub fn opt_level(self, level: impl Into<Value>) -> Self {
        self.set("opt-level", level)
    }

    /// `"none"`, `"debuginfo"`, `"symbols"`, `true` or `false`.
    pub fn strip(self, strip: impl Into<Value>) -> Self {
        self.set("strip", strip)
    }

    pub fn set(mut self, key: &str, setting: impl Into<Value>) -> Self {
        self.table.insert(key, value(setting));
        self
    }

    /// A setting for some packages only, such as `[profile.dev.package."*"]`
    /// for every dependency.
    pub fn package(mut self, package: &str, key: &str, setting: impl Into<Value>) -> Self {
        let packages = self.table.entry("package").or_insert_with(|| {
            let mut packages = Table::new();
            packages.set_implicit(true);
            Item::Table(packages)
        });
        let settings = packages.as_table_mut().map(|packages| {
            packages
                .entry(package)
                .or_insert_with(|| Item::Table(Table::new()))
        });
        if let Some(settings) = settings.and_then(Item::as_table_mut) {
            settings.insert(key, value(setting));
        }
        self
    }
}

/// The workspace settings in the root `Cargo.toml`: members, shared package
/// metadata, lints and profiles.
///
/// framer owns only these keys, and in `[workspace.package]` only the ones you
/// set. Everything else, including `version` and `[workspace.dependencies]`,
/// stays yours, so `cargo add` and release tools keep working.
#[derive(Clone, Debug)]
pub struct CargoWorkspace {
    resolver: Option<String>,
    members: Vec<String>,
    exclude: Vec<String>,
    package: Table,
    lints: Option<Lints>,
    profiles: Vec<Profile>,
}

impl Default for CargoWorkspace {
    fn default() -> Self {
        Self::new()
    }
}

impl CargoWorkspace {
    /// Resolver 3 with every crate under `crates/` as a member.
    pub fn new() -> Self {
        Self {
            resolver: Some("3".into()),
            members: vec!["crates/*".into()],
            exclude: Vec::new(),
            package: Table::new(),
            lints: None,
            profiles: Vec::new(),
        }
    }

    pub fn resolver(mut self, resolver: Option<&str>) -> Self {
        self.resolver = resolver.map(Into::into);
        self
    }

    pub fn members<T: Into<String>>(mut self, members: impl IntoIterator<Item = T>) -> Self {
        self.members = members.into_iter().map(Into::into).collect();
        self
    }

    pub fn exclude<T: Into<String>>(mut self, paths: impl IntoIterator<Item = T>) -> Self {
        self.exclude = paths.into_iter().map(Into::into).collect();
        self
    }

    pub fn edition(self, edition: Edition) -> Self {
        self.package("edition", edition.as_str())
    }

    pub fn rust_version(self, version: &str) -> Self {
        self.package("rust-version", version)
    }

    pub fn license(self, license: &str) -> Self {
        self.package("license", license)
    }

    pub fn repository(self, url: &str) -> Self {
        self.package("repository", url)
    }

    pub fn homepage(self, url: &str) -> Self {
        self.package("homepage", url)
    }

    pub fn authors<T: Into<String>>(self, authors: impl IntoIterator<Item = T>) -> Self {
        self.package("authors", array(authors))
    }

    /// Any other `[workspace.package]` key.
    pub fn package(mut self, key: &str, setting: impl Into<Value>) -> Self {
        self.package.insert(key, value(setting));
        self
    }

    pub fn lints(mut self, lints: Lints) -> Self {
        self.lints = Some(lints);
        self
    }

    /// The lints, for changing ones a preset set up.
    pub fn lints_mut(&mut self) -> &mut Lints {
        self.lints.get_or_insert_with(Lints::new)
    }

    /// Adds a profile, replacing any profile with the same name.
    pub fn profile(mut self, profile: Profile) -> Self {
        self.profiles
            .retain(|existing| existing.name != profile.name);
        self.profiles.push(profile);
        self
    }
}

impl Component for CargoWorkspace {
    fn synthesize(&self, output: &mut Output) -> Result<()> {
        if let Some(resolver) = &self.resolver {
            output.toml(CARGO_TOML, "workspace.resolver", value(resolver))?;
        }
        output.toml(CARGO_TOML, "workspace.members", value(array(&self.members)))?;
        if !self.exclude.is_empty() {
            output.toml(CARGO_TOML, "workspace.exclude", value(array(&self.exclude)))?;
        }
        // Owned key by key, so settings such as `version` and `publish` stay yours.
        for (key, item) in &self.package {
            let key = KeyPath::new(["workspace", "package", key]);
            output.toml(CARGO_TOML, key, item.clone())?;
        }
        if let Some(lints) = &self.lints {
            output.toml(CARGO_TOML, "workspace.lints", lints.to_item())?;
        }
        for profile in &self.profiles {
            let key = KeyPath::new(["profile", profile.name.as_str()]);
            output.toml(CARGO_TOML, key, Item::Table(profile.table.clone()))?;
        }
        Ok(())
    }
}
