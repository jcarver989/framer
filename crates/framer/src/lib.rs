//! Generate and maintain a Rust repository's configuration files from typed Rust code.
//!
//! A repository keeps a small crate in `.framer/` whose `main` builds a
//! [`Project`] and calls [`Project::synth`]. Running `cargo framer` writes every
//! file the project describes:
//!
//! - **Generated files** such as CI workflows and `rust-toolchain.toml` are
//!   owned by framer, start with a "do not edit" header and are read-only.
//! - **Partly generated files** keep everything framer doesn't own. framer only
//!   owns certain keys of a TOML file (such as `[workspace.lints]` in
//!   `Cargo.toml`) or marked blocks of a text file (such as shared conventions in
//!   `AGENTS.md`).
//! - **Sample files** are written once and then belong to you.
//!
//! `.framer/manifest.json` records what was generated, so files and keys that
//! are no longer generated get cleaned up, and `cargo framer --check` fails in
//! CI when someone edits a generated file by hand.
//!
//! ```no_run
//! use framer::Project;
//! use framer::github::{Ci, Job};
//! use framer::rust::{CargoWorkspace, Edition, Lints, Toolchain};
//! use std::process::ExitCode;
//!
//! fn main() -> ExitCode {
//!     let mut project = Project::new();
//!     project.add(Toolchain::pinned("1.98.0").components(["clippy", "rustfmt"]));
//!     project.add(
//!         CargoWorkspace::new()
//!             .edition(Edition::E2024)
//!             .rust_version("1.98")
//!             .lints(Lints::clippy_pedantic()),
//!     );
//!     project.add(Ci::new().job(Job::fmt()).job(Job::clippy()).job(Job::test()));
//!     project.synth()
//! }
//! ```
//!
//! To share a setup across repositories, write a [`Preset`] and publish it as a crate.

mod blocks;
mod error;
mod files;
mod format;
mod fs;
pub mod github;
mod manifest;
mod output;
mod project;
pub mod rust;
mod synth;
pub mod testing;
mod toml_patch;
pub mod yaml;

pub use error::{FramerError, Result};
pub use files::{SampleFile, TextBlock, TextFile, TomlFile, TomlKeys};
pub use format::CommentStyle;
pub use fs::{DiskFs, FileSystem, MemoryFs};
pub use output::Output;
pub use project::{Component, Preset, Project, TomlOverrides};
pub use synth::{ChangeKind, FileChange, Mode, Report, eject};
pub use toml_edit;
pub use toml_patch::KeyPath;
