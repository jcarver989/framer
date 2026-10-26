use framer::Project;
use framer_presets::{Ci, Rust, RustMonorepo};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut project = Project::new();
    project.apply(
        RustMonorepo::new()
            .rust(Rust::new("1.98"))
            .ci(Ci::new().matrix(["ubuntu-latest", "macos-latest"])),
    );
    project.synth()
}
