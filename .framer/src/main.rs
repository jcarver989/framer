use framer::Project;
use framer_presets::{Release, ReleasePlz, Rust, RustMonorepo, Workspace};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut project = Project::new();
    project.apply(
        RustMonorepo::new()
            .rust(Rust::new("1.98"))
            .workspace(Workspace::new().repository("https://github.com/jcarver989/framer"))
            .release(Release::new().release_plz(ReleasePlz::new().publish_to_crates_io())),
    );
    project.synth()
}
