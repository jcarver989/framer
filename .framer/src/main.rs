use framer::Project;
use framer_presets::RustMonorepo;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut project = Project::new();
    project.apply(
        RustMonorepo::new()
            .msrv("1.98")
            .ci_runners(["ubuntu-latest", "macos-latest"]),
    );
    project.synth()
}
