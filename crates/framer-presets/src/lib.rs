//! Ready-made [framer] presets.
//!
//! ```no_run
//! use framer::Project;
//! use framer_presets::{Dist, Release, ReleasePlz, RustMonorepo, Workspace};
//! use std::process::ExitCode;
//!
//! fn main() -> ExitCode {
//!     let mut project = Project::new();
//!     project.apply(
//!         RustMonorepo::new()
//!             .workspace(Workspace::new().repository("https://github.com/me/my-tool"))
//!             .release(
//!                 Release::new()
//!                     .release_plz(ReleasePlz::new())
//!                     .dist(Dist::new().homebrew_tap("me/homebrew-tap")),
//!             ),
//!     );
//!     project.synth()
//! }
//! ```

mod ci;
mod dist;
mod release;
mod release_plz;
mod rust;
mod rust_monorepo;
mod rustfmt;
mod tools;
mod workspace;

pub use ci::{Ci, CompilingJob};
pub use dist::Dist;
pub use release::Release;
pub use release_plz::ReleasePlz;
pub use rust::Rust;
pub use rust_monorepo::RustMonorepo;
pub use rustfmt::Rustfmt;
pub use tools::Tools;
pub use workspace::Workspace;
