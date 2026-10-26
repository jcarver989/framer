# framer-presets

Ready-made [framer](https://crates.io/crates/framer) presets for setting up Rust repositories.

```toml
# .framer/Cargo.toml
[dependencies]
framer = "0.1"
framer-presets = "0.1"
```

```rust
// .framer/src/main.rs
use framer::Project;
use framer_presets::{Dist, Release, ReleasePlz, RustMonorepo, Workspace};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut project = Project::new();
    project.apply(
        RustMonorepo::new()
            .workspace(Workspace::new().repository("https://github.com/me/my-tool"))
            .release(
                Release::new()
                    .release_plz(ReleasePlz::new())
                    .dist(Dist::new().homebrew_tap("me/homebrew-tap")),
            ),
    );
    project.synth()
}
```

## `RustMonorepo`

A Cargo workspace of crates under `crates/`, held to a high bar, with CI that runs the same `just` recipes you run locally.

| File | What's generated |
|---|---|
| `rust-toolchain.toml` | A pinned toolchain with Clippy and rustfmt. |
| `Cargo.toml` | Only these workspace settings: edition 2024, MSRV, license, repository, lints (Clippy `all` and `pedantic`, `unsafe_code` denied, `unreachable_pub` warned), and a `dev` profile that builds faster by keeping only line tables for your crates and no debug info for dependencies. Your `version`, dependencies and everything else stay yours. |
| `clippy.toml`, `rustfmt.toml` | The MSRV for Clippy; field-init and `?` shorthand for rustfmt. |
| `.config/nextest.toml` | A `default` profile and a `ci` profile that writes JUnit results. |
| `.tool-versions` | `just` and `cargo-nextest`, installed by [mise](https://mise.jdx.dev). |
| `justfile` | A block of recipes: `check`, `test`, `test-ci`, `lint`, `fmt`, `fmt-check`, `doc-check`, `framer` and `framer-check`. Keep your own recipes outside it. |
| `.github/workflows/ci.yml` | Formatting, Clippy, tests, docs and the generated-files check, each running its `just` recipe. |
| `.gitignore`, `AGENTS.md` | Blocks with the usual ignores, and coding and testing conventions for people and agents. |

Options are grouped by theme. Each setter takes one config struct whose `new()` holds the preset's defaults (`Rust::new` takes just the MSRV), and passing one replaces that theme:

- `rust(Rust::new("1.98").toolchain("1.99.1").targets([...]))`: the MSRV, used in `Cargo.toml` and `clippy.toml`, the pinned toolchain (the MSRV's first release by default) and extra targets.
- `workspace(Workspace::new())`: `members([...])`, `exclude([...])`, `extra_manifests([...])` for manifests outside the workspace that `lint` and `fmt` also cover, `license(...)`, `repository(...)` and `profile(Profile)`.
- `lints(Lints)`: replaces the preset's lints.
- `rustfmt(Rustfmt::new().set("max_width", 120))`: `rustfmt.toml` options on top of the preset's.
- `tools(Tools::new().tool("actionlint", "1.7.12"))`: `.tool-versions`. `just(..)` and `nextest(..)` change the versions the recipes run; the nextest version is also required in `.config/nextest.toml`.
- `ci(Ci::new())`: `matrix([...])` runs tests (and any `CompilingJob` with `runs_on_matrix()`) on more operating systems, `setup(step)` runs a step such as installing system libraries before every job that compiles, `compiling_job(CompilingJob::new(id, name, recipe))` adds a job that sets up like the preset's own, `job(Job)` adds any other job, and `without_framer_check()` leaves the generated-files check out. `without_ci()` generates no workflow.
- `release(Release::new())`: release automation, off by default, with these two:
  - `release_plz(ReleasePlz::new())`: release PRs and tags with [release-plz](https://release-plz.dev). framer owns the `[workspace]` settings in `release-plz.toml` and leaves `[[package]]` entries to you. The workflow uses a GitHub App token from the `RELEASE_AUTOMATION_APP_ID` and `RELEASE_AUTOMATION_APP_PRIVATE_KEY` secrets, so the tags it pushes can start the release workflow. Use `.publish_to_crates_io()` to publish crates through crates.io [trusted publishing](https://crates.io/docs/trusted-publishing), with no registry token: add a trusted publisher for `release-plz.yml` to each crate on crates.io, after publishing new crates once by hand. Use `.github_token()` to skip the app.
  - `dist(Dist::new())`: binary releases with [dist](https://opensource.axo.dev/cargo-dist/). Generates `dist-workspace.toml`, `[profile.dist]`, `just dist-*` recipes and a CI check that dist's `release.yml` matches the config. dist itself still generates `release.yml`: run `just dist-generate` after changing the config. Crates opt in with `[package.metadata.dist] dist = true`.

To change anything else, use framer's escape hatches: `project.get_mut::<T>()`, `project.toml(path).set(...)` or `project.unmanage(path)`.

## Notes

- The justfile block includes `set shell` and a `default` recipe. If your justfile already has either, remove yours. `just` with no arguments runs the first recipe in the file, so put the block first if you want `default` to list recipes.
