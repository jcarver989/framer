//! Ready-made [framer] presets.
//!
//! ```no_run
//! use framer::Project;
//! use framer_presets::{Dist, ReleasePlz, RustMonorepo};
//! use std::process::ExitCode;
//!
//! fn main() -> ExitCode {
//!     let mut project = Project::new();
//!     project.apply(
//!         RustMonorepo::new()
//!             .repository("https://github.com/me/my-tool")
//!             .release_plz(ReleasePlz::new())
//!             .dist(Dist::new().homebrew_tap("me/homebrew-tap")),
//!     );
//!     project.synth()
//! }
//! ```

mod ci;
mod dist;
mod release_plz;
mod rust_monorepo;

pub use dist::Dist;
pub use release_plz::ReleasePlz;
pub use rust_monorepo::RustMonorepo;
