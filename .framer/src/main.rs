use framer::Project;
use framer::toml_edit::Array;
use framer_presets::{Release, ReleasePlz, Rust, RustMonorepo, Tools, Workspace};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut project = Project::new();
    project.apply(
        RustMonorepo::new()
            .rust(Rust::new("1.100"))
            .tools(Tools::new().mise_toml())
            .workspace(
                Workspace::new()
                    .members(["crates/*", ".framer"])
                    .repository("https://github.com/jcarver989/framer"),
            )
            .release(Release::new().release_plz(ReleasePlz::new().publish_to_crates_io())),
    );
    project
        .toml("mise.toml")
        .set("min_version", "2026.11.10")
        .set(
            "settings.lockfile_platforms",
            Array::from_iter(["linux-x64", "linux-arm64", "macos-arm64", "macos-x64"]),
        )
        .set("tool_config.locked", true)
        .set(
            "tasks.tools-check.description",
            "Check the mise tool lockfile",
        )
        .set("tasks.tools-check.run", "mise lock --check")
        .set(
            "tasks.framer-check.depends",
            Array::from_iter(["tools-check"]),
        );
    project.synth()
}
