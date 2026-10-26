use crate::{Ci, Dist, Release, Rust, Rustfmt, Tools, Workspace};
use framer::just::{Justfile, Recipe};
use framer::rust::{CargoWorkspace, Level, Lints};
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
/// - The workspace settings in `Cargo.toml`: members, edition 2024, the MSRV,
///   the license, lints (Clippy `all` and `pedantic`, `unsafe_code` denied,
///   `unreachable_pub` warned) and a faster-building `dev` profile. Dependencies
///   stay yours.
/// - `clippy.toml`, `rustfmt.toml` and `.config/nextest.toml`, which requires
///   the nextest version pinned in `.tool-versions`.
/// - `.tool-versions`, so mise installs `just` and `cargo-nextest`.
/// - A [`Justfile`] block of standard recipes: `check`, `test`, `test-ci`,
///   `lint`, `fmt`, `fmt-check`, `doc-check`, `framer` and `framer-check`.
///   Change them with `project.get_mut::<Justfile>()`, and keep your own
///   recipes outside the block.
/// - `.github/workflows/ci.yml`, running formatting, Clippy, tests, docs and
///   the generated-files check through those recipes.
/// - Blocks in `.gitignore` and `AGENTS.md`, the latter with coding and
///   testing conventions for people and agents.
///
/// Each theme is configured with one setter taking one config struct, whose
/// `new()` holds the preset's defaults: [`Rust`], [`Workspace`], [`Lints`],
/// [`Rustfmt`], [`Tools`] and [`Ci`]. [`Rust::new`] takes just the MSRV.
/// Passing one replaces that theme.
///
/// Release automation is opt-in, with [`RustMonorepo::release`].
#[derive(Clone, Debug)]
pub struct RustMonorepo {
    rust: Rust,
    workspace: Workspace,
    lints: Lints,
    rustfmt: Rustfmt,
    tools: Tools,
    ci: Option<Ci>,
    conventions: bool,
    release: Release,
}

impl Default for RustMonorepo {
    fn default() -> Self {
        Self::new()
    }
}

impl RustMonorepo {
    pub fn new() -> Self {
        Self {
            rust: Rust::default(),
            workspace: Workspace::new(),
            lints: Lints::clippy_pedantic()
                .allow_clippy([
                    "missing_errors_doc",
                    "missing_panics_doc",
                    "must_use_candidate",
                    "return_self_not_must_use",
                ])
                .rust("unsafe_code", Level::Deny)
                .rust("unreachable_pub", Level::Warn),
            rustfmt: Rustfmt::new(),
            tools: Tools::new(),
            ci: Some(Ci::new()),
            conventions: true,
            release: Release::new(),
        }
    }

    pub fn rust(mut self, rust: Rust) -> Self {
        self.rust = rust;
        self
    }

    pub fn workspace(mut self, workspace: Workspace) -> Self {
        self.workspace = workspace;
        self
    }

    /// Replaces the preset's lints.
    pub fn lints(mut self, lints: Lints) -> Self {
        self.lints = lints;
        self
    }

    pub fn rustfmt(mut self, rustfmt: Rustfmt) -> Self {
        self.rustfmt = rustfmt;
        self
    }

    pub fn tools(mut self, tools: Tools) -> Self {
        self.tools = tools;
        self
    }

    pub fn ci(mut self, ci: Ci) -> Self {
        self.ci = Some(ci);
        self
    }

    /// Generates no CI workflow, for repositories whose CI can't build them,
    /// or that write their own.
    pub fn without_ci(mut self) -> Self {
        self.ci = None;
        self
    }

    /// Leaves the conventions block out of `AGENTS.md`, for repositories that
    /// write their own.
    pub fn without_conventions(mut self) -> Self {
        self.conventions = false;
        self
    }

    pub fn release(mut self, release: Release) -> Self {
        self.release = release;
        self
    }

    fn cargo_workspace(&self) -> CargoWorkspace {
        let workspace = self
            .workspace
            .cargo_workspace(self.rust.msrv(), self.lints.clone());
        match self.release.dist_config() {
            Some(_) => workspace.profile(Dist::profile()),
            None => workspace,
        }
    }

    fn tool_versions(&self) -> TextFile {
        let tools = self
            .release
            .dist_config()
            .map(Dist::tool)
            .into_iter()
            .fold(self.tools.clone(), Tools::with_default);
        TextFile::new(".tool-versions", tools.file_contents())
    }

    fn nextest_config(&self) -> TomlFile {
        let mut required = InlineTable::new();
        required.insert("required", self.tools.nextest_version().into());
        let mut slow_timeout = InlineTable::new();
        slow_timeout.insert("period", "60s".into());
        slow_timeout.insert("terminate-after", 5.into());
        TomlFile::new(".config/nextest.toml")
            .set("nextest-version", Value::InlineTable(required))
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

    fn justfile(&self) -> Justfile {
        let manifests = self.workspace.extra_manifest_paths();
        let mut lint = Recipe::new("lint")
            .doc("Run Clippy, failing on any warning")
            .line("cargo clippy --workspace --all-targets --all-features -- -D warnings");
        let mut fmt = Recipe::new("fmt")
            .doc("Format all code")
            .line("cargo fmt --all");
        let mut fmt_check = Recipe::new("fmt-check")
            .doc("Check formatting")
            .line("cargo fmt --all -- --check");
        for manifest in manifests {
            lint = lint.line(format!(
                "cargo clippy --manifest-path {manifest} --target-dir target --all-targets --all-features -- -D warnings"
            ));
            fmt = fmt.line(format!("cargo fmt --manifest-path {manifest}"));
            fmt_check = fmt_check.line(format!("cargo fmt --manifest-path {manifest} -- --check"));
        }
        let recipes = [
            Recipe::new("default")
                .doc("List available recipes")
                .line("@just --list"),
            Recipe::new("check")
                .doc("Type-check every crate and target")
                .line("cargo check --workspace --all-targets --all-features"),
            Recipe::new("test")
                .doc("Run tests with nextest, then doctests, which nextest doesn't run")
                .parameters(r#"profile="default""#)
                .line("cargo nextest run --profile {{profile}} --workspace --all-features --no-tests pass")
                .line("cargo test --doc --workspace --all-features"),
            Recipe::new("test-ci")
                .doc("Run tests with nextest's CI profile, which writes JUnit results")
                .dependencies(r#"(test "ci")"#),
            lint,
            fmt,
            fmt_check,
            Recipe::new("doc-check")
                .doc("Build docs, failing on any warning")
                .line(r#"RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps --document-private-items"#),
            Recipe::new("framer")
                .doc("Regenerate files from .framer/")
                .line("cargo run --quiet --manifest-path .framer/Cargo.toml"),
            Recipe::new("framer-check")
                .doc("Check generated files are up to date")
                .line("cargo run --quiet --locked --manifest-path .framer/Cargo.toml -- --check"),
        ];
        let dist_recipes = self
            .release
            .dist_config()
            .into_iter()
            .flat_map(|_| Dist::recipes());
        recipes.into_iter().chain(dist_recipes).fold(
            Justfile::new(BLOCK).setting(r#"shell := ["bash", "-euo", "pipefail", "-c"]"#),
            Justfile::recipe,
        )
    }
}

impl Preset for RustMonorepo {
    fn apply(self, project: &mut Project) {
        project.add(self.rust.toolchain_file());
        project.add(self.cargo_workspace());
        project.add(TomlFile::new("clippy.toml").set("msrv", self.rust.msrv()));
        project.add(self.rustfmt.file());
        project.add(self.nextest_config());
        project.add(self.tool_versions());
        project.add(self.justfile());
        if let Some(ci) = &self.ci {
            let workflow = ci.workflow();
            project.add(match self.release.dist_config().and_then(Dist::ci_job) {
                Some(job) => workflow.job(job),
                None => workflow,
            });
        }
        project.add(TextBlock::new(
            ".gitignore",
            BLOCK,
            "/target\n**/*.rs.bk\n.DS_Store\n",
        ));
        if self.conventions {
            project.add(TextBlock::new(
                "AGENTS.md",
                format!("{BLOCK}/conventions"),
                CONVENTIONS,
            ));
        }
        self.release.apply(project);
    }
}
