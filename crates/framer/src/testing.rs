//! Helpers for testing projects and presets without touching the disk.
//!
//! ```
//! use framer::rust::Toolchain;
//! use framer::{Preset, Project};
//!
//! struct MyRust;
//!
//! impl Preset for MyRust {
//!     fn apply(self, project: &mut Project) {
//!         project.add(Toolchain::pinned("1.98.0"));
//!     }
//! }
//!
//! let files = framer::testing::render_preset(MyRust).unwrap();
//! assert!(files.get("rust-toolchain.toml").unwrap().contains("channel = \"1.98.0\""));
//! ```

use crate::error::Result;
use crate::fs::MemoryFs;
use crate::project::{Preset, Project};
use crate::synth::Mode;

/// Generates a project into an empty in-memory repository.
pub fn render(project: &Project) -> Result<MemoryFs> {
    render_into(project, MemoryFs::new())
}

/// Generates a project into an in-memory repository that already has some files.
pub fn render_into(project: &Project, mut fs: MemoryFs) -> Result<MemoryFs> {
    project.synth_with(&mut fs, Mode::Write)?;
    Ok(fs)
}

/// Applies a preset to a new project and generates it into an empty in-memory repository.
pub fn render_preset<T: Preset>(preset: T) -> Result<MemoryFs> {
    let mut project = Project::new();
    project.apply(preset);
    render(&project)
}
