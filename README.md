# framer

Generate and maintain your Rust repository's configuration (CI workflows, `Cargo.toml` workspace settings, toolchain pins, shared docs) from typed Rust code. Inspired by [projen](https://projen.io).

```rust
// .framer/src/main.rs
use framer::Project;
use framer::github::{Ci, Job};
use framer::rust::{CargoWorkspace, Edition, Lints, Toolchain};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut project = Project::new();
    project.add(Toolchain::pinned("1.98.0").components(["clippy", "rustfmt"]));
    project.add(
        CargoWorkspace::new()
            .edition(Edition::E2024)
            .rust_version("1.98")
            .lints(Lints::clippy_pedantic().allow_clippy(["missing_errors_doc"])),
    );
    project.add(
        Ci::new()
            .matrix(["ubuntu-latest", "macos-latest"])
            .job(Job::fmt())
            .job(Job::clippy())
            .job(Job::test()),
    );
    project.synth()
}
```

```console
$ cargo framer
  + .github/workflows/ci.yml (created)
  ~ Cargo.toml (updated)
  + rust-toolchain.toml (created)
  + .framer/manifest.json (created)
```

## Getting started

```console
$ cargo install cargo-framer
$ cd my-repo
$ cargo framer init        # creates .framer/ and runs it
```

Edit `.framer/src/main.rs`, then run `cargo framer` again. Commit `.framer/` (including `Cargo.lock` and `manifest.json`) along with the generated files.

## How files are generated

| Kind | Examples | What framer does |
|---|---|---|
| **Generated** | CI workflows, `rust-toolchain.toml`, `TomlFile`, `TextFile` | Owns the whole file. Adds a "do not edit" header and makes it read-only. |
| **Partly generated** | `Cargo.toml`, `AGENTS.md`, `.gitignore` | Owns only certain TOML keys (such as `[workspace.lints]`) or marked blocks. Leaves the rest, including its formatting and comments, to you. |
| **Sample** | `SampleFile` | Writes the file once if it's missing, then never touches it again. |

`CargoWorkspace` owns the workspace members, resolver, `[workspace.package]`, `[workspace.lints]` and any profiles you add. It doesn't touch `[workspace.dependencies]`, so `cargo add` keeps working.

`.framer/manifest.json` records what each run generated. When you stop generating something, the next run deletes the file, removes the TOML keys, or removes the block.

## Keeping generated files honest

`Ci` adds a job that runs `cargo framer --check`, which fails with a diff if anyone edited a generated file by hand or forgot to run `cargo framer` after changing `.framer/`. Leave it out with `Ci::without_framer_check()`.

## Sharing a setup with presets

A preset is a crate that configures a project, so a team's "favorite way to set up a Rust repo" lives in one versioned place:

```rust
use framer::github::{Ci, Job};
use framer::rust::{CargoWorkspace, Edition, Lints, Toolchain};
use framer::{Preset, Project, TextBlock};

pub struct TeamRust {
    msrv: String,
}

impl TeamRust {
    pub fn new() -> Self {
        Self { msrv: "1.98".into() }
    }

    pub fn msrv(mut self, msrv: &str) -> Self {
        self.msrv = msrv.into();
        self
    }
}

impl Preset for TeamRust {
    fn apply(self, project: &mut Project) {
        project.add(Toolchain::pinned(format!("{}.0", self.msrv)).components(["clippy", "rustfmt"]));
        project.add(
            CargoWorkspace::new()
                .edition(Edition::E2024)
                .rust_version(&self.msrv)
                .lints(Lints::clippy_pedantic()),
        );
        project.add(Ci::new().job(Job::fmt()).job(Job::clippy()).job(Job::test()));
        project.add(TextBlock::new("AGENTS.md", "team/conventions", include_str!("conventions.md")));
    }
}
```

Each repository applies the preset and changes what it needs to:

```rust
let mut project = Project::new();
project.apply(TeamRust::new());
project.get_mut::<Ci>().unwrap().job_mut("test").unwrap().steps_mut().push(Step::run("just e2e"));
project.synth()
```

For a ready-made one, see [`framer-presets`](crates/framer-presets): `project.apply(framer_presets::RustMonorepo::new())`.

`RustMonorepo` covers the common variations without escape hatches:

```rust
project.apply(
    RustMonorepo::new()
        .workspace(
            Workspace::new()
                .members(["crates/ui/*", "crates/app/*"])
                .exclude(["vendor/gpui"])
                .extra_manifests(["tests/fixture/Cargo.toml"]),
        )
        .rustfmt(Rustfmt::new().set("max_width", 120))
        .ci(
            Ci::new()
                .compiling_job(CompilingJob::new("wasm", "WASM", "wasm-check"))
                .without_framer_check(),
        ),
);
```

Each theme takes one config struct whose `new()` holds the preset's defaults (`Rust::new` takes just the MSRV): `.rust(Rust)`, `.workspace(Workspace)`, `.lints(Lints)`, `.rustfmt(Rustfmt)`, `.tools(Tools)`, `.ci(Ci)` and `.release(Release)`. It also takes `.without_ci()` and `.without_conventions()`. A repository can also publish its own preset from its `.framer/` crate, as a library next to the binary, for sibling repositories to depend on by path.

Bumping the preset's version in a repository and running `cargo framer` rolls out the change. Test presets without touching the disk using `framer::testing::render_preset`, which generates into an in-memory `MemoryFs`.

## Escape hatches

- **Change a component a preset added:** `project.get_mut::<T>()`.
- **Change a recipe a preset added:** `project.get_mut::<framer::just::Justfile>()`, then `recipe_mut("lint")` or `remove_recipe("test")`. Recipes outside the block stay yours.
- **Set any TOML key**, in a generated or partly generated file: `project.toml("Cargo.toml").set("profile.dist.codegen-units", 1_i64)`.
- **Stop generating one file** but keep it as yours: `project.unmanage(".github/workflows/ci.yml")`.
- **Write your own component:** implement `Component` and write to the `Output`.
- **Stop using framer:** `cargo framer eject` strips the headers and markers and leaves ordinary files.

## Repository layout

- `crates/framer`: the library.
- `crates/cargo-framer`: the `cargo framer` command, which builds and runs `.framer/`.
- `crates/framer-presets`: ready-made presets, starting with `RustMonorepo`.
- `.framer/`: this repository's own framer project. It generates this repository's CI, toolchain pin, `Cargo.toml` workspace settings and the conventions in `AGENTS.md`.
