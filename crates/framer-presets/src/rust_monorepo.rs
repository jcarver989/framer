use crate::ci::{compiling_job, just_job};
use crate::{Dist, ReleasePlz};
use framer::github::{Step, Triggers, Workflow};
use framer::rust::{CargoWorkspace, Edition, Level, Lints, Profile, Toolchain};
use framer::toml_edit::{InlineTable, Value};
use framer::{Preset, Project, TextBlock, TextFile, TomlFile};

const BLOCK: &str = "rust-monorepo";
const CONVENTIONS: &str = include_str!("conventions.md");

/// A Cargo workspace of crates under `crates/`, held to a high bar: pedantic
/// Clippy, a pinned toolchain, nextest, and CI that runs the same `just`
/// recipes you run locally.
///
/// It generates:
///
/// - `rust-toolchain.toml`, pinned, with Clippy and rustfmt.
/// - The workspace settings in `Cargo.toml`: edition 2024, the MSRV, the
///   license, lints (Clippy `all` and `pedantic`, `unsafe_code` denied,
///   `unreachable_pub` warned) and a faster-building `dev` profile. Dependencies
///   stay yours.
/// - `clippy.toml`, `rustfmt.toml` and `.config/nextest.toml`.
/// - `.tool-versions`, so mise installs `just` and `cargo-nextest`.
/// - A block of standard recipes in `justfile`: `check`, `test`, `test-ci`,
///   `lint`, `fmt`, `fmt-check`, `doc-check`, `framer` and `framer-check`.
///   Keep your own recipes outside the block.
/// - `.github/workflows/ci.yml`, running formatting, Clippy, tests, docs and
///   the generated-files check through those recipes.
/// - Blocks in `.gitignore` and `AGENTS.md`, the latter with coding and
///   testing conventions for people and agents.
///
/// Release automation is opt-in, with [`RustMonorepo::release_plz`] and [`RustMonorepo::dist`].
#[derive(Clone, Debug)]
pub struct RustMonorepo {
    msrv: String,
    toolchain: Option<String>,
    targets: Vec<String>,
    license: String,
    repository: Option<String>,
    tools: Vec<(String, String)>,
    ci_runners: Vec<String>,
    ci_setup: Vec<Step>,
    release_plz: Option<ReleasePlz>,
    dist: Option<Dist>,
}

impl Default for RustMonorepo {
    fn default() -> Self {
        Self::new()
    }
}

impl RustMonorepo {
    pub fn new() -> Self {
        Self {
            msrv: "1.98".into(),
            toolchain: None,
            targets: Vec::new(),
            license: "MIT".into(),
            repository: None,
            tools: vec![
                ("just".into(), "1.58.0".into()),
                (
                    "aqua:nextest-rs/nextest/cargo-nextest".into(),
                    "0.9.144".into(),
                ),
            ],
            ci_runners: vec!["ubuntu-latest".into()],
            ci_setup: Vec::new(),
            release_plz: None,
            dist: None,
        }
    }

    /// The minimum supported Rust version, such as `1.98`. Also pins the
    /// toolchain to its first release, unless [`RustMonorepo::toolchain`] says otherwise.
    pub fn msrv(mut self, version: &str) -> Self {
        version.clone_into(&mut self.msrv);
        self
    }

    /// The exact toolchain to develop and test with, such as `1.99.1`.
    pub fn toolchain(mut self, version: &str) -> Self {
        self.toolchain = Some(version.into());
        self
    }

    /// Extra compilation targets to install, such as `wasm32-unknown-unknown`.
    pub fn targets<T: Into<String>>(mut self, triples: impl IntoIterator<Item = T>) -> Self {
        self.targets.extend(triples.into_iter().map(Into::into));
        self
    }

    /// The SPDX license expression, `MIT` by default.
    pub fn license(mut self, license: &str) -> Self {
        license.clone_into(&mut self.license);
        self
    }

    pub fn repository(mut self, url: &str) -> Self {
        self.repository = Some(url.into());
        self
    }

    /// Adds a tool to `.tool-versions`, or changes the version of one the
    /// preset installs, such as `just`.
    pub fn tool(mut self, name: &str, version: &str) -> Self {
        upsert(&mut self.tools, name, version);
        self
    }

    /// The runners tests run on, `ubuntu-latest` by default.
    pub fn ci_runners<T: Into<String>>(mut self, runners: impl IntoIterator<Item = T>) -> Self {
        self.ci_runners = runners.into_iter().map(Into::into).collect();
        self
    }

    /// A step to run before building in CI, such as installing system libraries.
    pub fn ci_setup(mut self, step: Step) -> Self {
        self.ci_setup.push(step);
        self
    }

    pub fn release_plz(mut self, release_plz: ReleasePlz) -> Self {
        self.release_plz = Some(release_plz);
        self
    }

    pub fn dist(mut self, dist: Dist) -> Self {
        self.dist = Some(dist);
        self
    }

    fn workspace(&self) -> CargoWorkspace {
        let lints = Lints::clippy_pedantic()
            .allow_clippy([
                "missing_errors_doc",
                "missing_panics_doc",
                "must_use_candidate",
                "return_self_not_must_use",
            ])
            .rust("unsafe_code", Level::Deny)
            .rust("unreachable_pub", Level::Warn);
        let mut workspace = CargoWorkspace::new()
            .edition(Edition::E2024)
            .rust_version(&self.msrv)
            .license(&self.license)
            .lints(lints)
            .profile(
                Profile::new("dev")
                    .set("debug", "line-tables-only")
                    .package("*", "debug", false),
            );
        if let Some(url) = &self.repository {
            workspace = workspace.repository(url);
        }
        if self.dist.is_some() {
            workspace = workspace.profile(Dist::profile());
        }
        workspace
    }

    fn tool_versions(&self) -> String {
        let mut tools = self.tools.clone();
        if let Some(dist) = &self.dist {
            let (name, version) = dist.tool();
            if !tools.iter().any(|(existing, _)| *existing == name) {
                tools.push((name, version));
            }
        }
        let mut lines: Vec<String> = tools
            .iter()
            .map(|(name, version)| format!("{name} {version}"))
            .collect();
        lines.push(String::new());
        lines.join("\n")
    }

    fn justfile(&self) -> String {
        let mut recipes = RECIPES.to_owned();
        if self.dist.is_some() {
            recipes.push_str(Dist::recipes());
        }
        recipes
    }

    fn ci(&self) -> Workflow {
        let setup = &self.ci_setup;
        let test = compiling_job("test", "Test", "test-ci", 30, setup)
            .runs_on_matrix()
            .step(
                Step::uses("actions/upload-artifact@v6")
                    .name("Upload test results")
                    .condition("always()")
                    .with("name", "test-results-${{ runner.os }}")
                    .with("path", "target/nextest/ci/junit.xml")
                    .with("if-no-files-found", "ignore"),
            );
        let mut workflow = Workflow::new("ci", "CI")
            .on(Triggers::new().pull_request().push(["main"]))
            .env("CARGO_INCREMENTAL", "0")
            .permission("contents", "read")
            .matrix(self.ci_runners.clone())
            .job(just_job("fmt", "Formatting", "fmt-check", 15))
            .job(compiling_job("lint", "Clippy", "lint", 20, setup))
            .job(test)
            .job(compiling_job("docs", "Docs", "doc-check", 15, setup))
            .job(just_job("framer", "Generated files", "framer-check", 15));
        if let Some(job) = self.dist.as_ref().and_then(Dist::ci_job) {
            workflow = workflow.job(job);
        }
        workflow
    }
}

impl Preset for RustMonorepo {
    fn apply(self, project: &mut Project) {
        let toolchain = self
            .toolchain
            .clone()
            .unwrap_or_else(|| format!("{}.0", self.msrv));
        project.add(
            Toolchain::pinned(toolchain)
                .components(["clippy", "rustfmt"])
                .targets(self.targets.clone()),
        );
        project.add(self.workspace());
        project.add(TomlFile::new("clippy.toml").set("msrv", self.msrv.as_str()));
        project.add(
            TomlFile::new("rustfmt.toml")
                .set("edition", "2024")
                .set("use_field_init_shorthand", true)
                .set("use_try_shorthand", true),
        );
        project.add(nextest_config());
        project.add(TextFile::new(".tool-versions", self.tool_versions()));
        project.add(TextBlock::new("justfile", BLOCK, self.justfile()));
        project.add(self.ci());
        project.add(TextBlock::new(
            ".gitignore",
            BLOCK,
            "/target\n**/*.rs.bk\n.DS_Store\n",
        ));
        project.add(TextBlock::new(
            "AGENTS.md",
            format!("{BLOCK}/conventions"),
            CONVENTIONS,
        ));
        if let Some(release_plz) = &self.release_plz {
            release_plz.apply(project);
        }
        if let Some(dist) = &self.dist {
            project.add(dist.config());
        }
    }
}

const RECIPES: &str = r#"set shell := ["bash", "-euo", "pipefail", "-c"]

# List available recipes
default:
    @just --list

# Type-check every crate and target
check:
    cargo check --workspace --all-targets --all-features

# Run tests with nextest, then doctests, which nextest doesn't run
test profile="default":
    cargo nextest run --profile {{profile}} --workspace --all-features --no-tests pass
    cargo test --doc --workspace --all-features

# Run tests with nextest's CI profile, which writes JUnit results
test-ci: (test "ci")

# Run Clippy, failing on any warning
lint:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Format all code
fmt:
    cargo fmt --all

# Check formatting
fmt-check:
    cargo fmt --all -- --check

# Build docs, failing on any warning
doc-check:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps --document-private-items

# Regenerate files from .framer/
framer:
    cargo run --quiet --manifest-path .framer/Cargo.toml

# Check generated files are up to date
framer-check:
    cargo run --quiet --locked --manifest-path .framer/Cargo.toml -- --check
"#;

fn nextest_config() -> TomlFile {
    let mut slow_timeout = InlineTable::new();
    slow_timeout.insert("period", "60s".into());
    slow_timeout.insert("terminate-after", 5.into());
    TomlFile::new(".config/nextest.toml")
        .set("profile.default.fail-fast", false)
        .set("profile.default.failure-output", "immediate-final")
        .set("profile.default.success-output", "never")
        .set("profile.default.retries", 0)
        .set(
            "profile.default.slow-timeout",
            Value::InlineTable(slow_timeout),
        )
        .set("profile.ci.inherits", "default")
        .set("profile.ci.junit.path", "junit.xml")
        .set("profile.ci.junit.store-success-output", false)
        .set("profile.ci.junit.store-failure-output", true)
}

fn upsert(tools: &mut Vec<(String, String)>, name: &str, version: &str) {
    match tools.iter_mut().find(|(existing, _)| existing == name) {
        Some(tool) => version.clone_into(&mut tool.1),
        None => tools.push((name.into(), version.into())),
    }
}
