use crate::error::Result;
use crate::files::TextBlock;
use crate::fs::{DiskFs, FileSystem};
use crate::output::Output;
use crate::synth::{self, Mode, Report};
use crate::toml_patch::KeyPath;
use std::any::Any;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use toml_edit::Value;

/// Something that generates files: a GitHub workflow, a Cargo workspace's settings, a block of text.
///
/// Components are plain Rust values. They're configured when added to a
/// [`Project`], can be found and changed again later with [`Project::get_mut`],
/// and only write their files when the project is synthesized.
pub trait Component: Any {
    fn synthesize(&self, output: &mut Output) -> Result<()>;
}

/// A reusable way to set up a project, such as "my favorite way to set up a Rust CLI".
///
/// Presets add and configure components. Publish one as a crate and every
/// repository that applies it picks up changes with a dependency bump.
pub trait Preset {
    fn apply(self, project: &mut Project);
}

/// Everything framer generates for a repository.
pub struct Project {
    components: Vec<Box<dyn Component>>,
    toml_overrides: BTreeMap<PathBuf, Vec<(KeyPath, Value)>>,
    unmanaged: BTreeSet<PathBuf>,
}

impl Default for Project {
    fn default() -> Self {
        Self::new()
    }
}

fn as_any(component: &dyn Component) -> &dyn Any {
    component
}

fn as_any_mut(component: &mut dyn Component) -> &mut dyn Any {
    component
}

impl Project {
    /// A project that only keeps `.framer/target/` out of git.
    pub fn new() -> Self {
        let mut project = Self {
            components: Vec::new(),
            toml_overrides: BTreeMap::new(),
            unmanaged: BTreeSet::new(),
        };
        project.add(TextBlock::new(".gitignore", "framer", "/.framer/target/\n"));
        project
    }

    pub fn add<T: Component>(&mut self, component: T) -> &mut T {
        self.components.push(Box::new(component));
        let last = self
            .components
            .last_mut()
            .expect("a component was just added");
        as_any_mut(&mut **last)
            .downcast_mut()
            .expect("the component just added is a T")
    }

    pub fn apply<T: Preset>(&mut self, preset: T) -> &mut Self {
        preset.apply(self);
        self
    }

    /// The first component of type `T`.
    pub fn get<T: Component>(&self) -> Option<&T> {
        self.components
            .iter()
            .find_map(|component| as_any(&**component).downcast_ref())
    }

    /// The first component of type `T`, to change something a preset set up.
    pub fn get_mut<T: Component>(&mut self) -> Option<&mut T> {
        self.components
            .iter_mut()
            .find_map(|component| as_any_mut(&mut **component).downcast_mut())
    }

    /// The first component of type `T`, adding a default one if there isn't one yet.
    pub fn get_or_add<T: Component + Default>(&mut self) -> &mut T {
        let position = self
            .components
            .iter()
            .position(|component| as_any(&**component).is::<T>());
        match position {
            Some(index) => as_any_mut(&mut *self.components[index])
                .downcast_mut()
                .expect("the component at this position is a T"),
            None => self.add(T::default()),
        }
    }

    /// Every component of type `T`, such as every GitHub workflow.
    pub fn all_mut<T: Component>(&mut self) -> impl Iterator<Item = &mut T> {
        self.components
            .iter_mut()
            .filter_map(|component| as_any_mut(&mut **component).downcast_mut())
    }

    /// Sets individual keys in a generated or partly generated TOML file, for
    /// settings no component has an option for.
    ///
    /// ```
    /// # let mut project = framer::Project::new();
    /// project.toml("Cargo.toml").set("profile.dist.codegen-units", 1_i64);
    /// ```
    pub fn toml(&mut self, path: impl Into<PathBuf>) -> TomlOverrides<'_> {
        TomlOverrides {
            overrides: self.toml_overrides.entry(path.into()).or_default(),
        }
    }

    /// Stops generating a file. If an earlier run generated it, it's kept as an
    /// ordinary file for the user to edit.
    pub fn unmanage(&mut self, path: impl Into<PathBuf>) -> &mut Self {
        self.unmanaged.insert(path.into());
        self
    }

    /// Generates every file into `fs`, or with [`Mode::Check`], reports what would change.
    pub fn synth_with(&self, fs: &mut dyn FileSystem, mode: Mode) -> Result<Report> {
        let mut output = Output::new();
        for component in &self.components {
            component.synthesize(&mut output)?;
        }
        synth::run(fs, output, &self.toml_overrides, &self.unmanaged, mode)
    }

    /// The entry point for `.framer/src/main.rs`.
    ///
    /// Generates every file in the repository at `$FRAMER_ROOT`, or the current
    /// directory. With `--check`, writes nothing and fails if any file is out of date.
    pub fn synth(&self) -> ExitCode {
        let args: Vec<String> = env::args().skip(1).collect();
        let mode = match args.as_slice() {
            [] => Mode::Write,
            [flag] if flag == "--check" => Mode::Check,
            _ => {
                eprintln!("usage: cargo framer [--check]");
                return ExitCode::from(2);
            }
        };
        let root = env::var_os("FRAMER_ROOT").map_or_else(|| PathBuf::from("."), PathBuf::from);
        if !root.join(".framer").is_dir() {
            eprintln!(
                "error: {} has no .framer/ directory; run framer from the repository root or with `cargo framer`",
                root.display()
            );
            return ExitCode::FAILURE;
        }

        match self.synth_with(&mut DiskFs::new(&root), mode) {
            Ok(report) if mode == Mode::Check && !report.is_clean() => {
                eprint!("{}", report.diff());
                eprintln!("error: generated files are out of date; run `cargo framer`");
                ExitCode::FAILURE
            }
            Ok(_) if mode == Mode::Check => {
                println!("Generated files are up to date.");
                ExitCode::SUCCESS
            }
            Ok(report) => {
                print!("{report}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("error: {error}");
                ExitCode::FAILURE
            }
        }
    }
}

/// Keys [`Project::toml`] sets in one TOML file.
pub struct TomlOverrides<'a> {
    overrides: &'a mut Vec<(KeyPath, Value)>,
}

impl TomlOverrides<'_> {
    pub fn set(self, key: impl Into<KeyPath>, value: impl Into<Value>) -> Self {
        self.overrides.push((key.into(), value.into()));
        self
    }
}
