use crate::tasks::TaskRunner;
use crate::{Ci, Dist, Release, Rust, Rustfmt, Tools, Workspace};
use framer::github::{Dependabot, DependabotUpdate};
use framer::rust::{CargoWorkspace, Level, Lints};
use framer::toml_edit::{InlineTable, Value};
use framer::{Preset, Project, TextBlock, TomlFile};

const BLOCK: &str = "rust-monorepo";
const CONVENTIONS: &str = include_str!("conventions.md");

/// A Cargo workspace of crates under `crates/`, held to a high bar: pedantic
/// Clippy, a pinned toolchain, nextest, and CI that runs the same tasks you
/// run locally.
///
/// It generates:
///
/// - `rust-toolchain.toml`, pinned, with Clippy and rustfmt.
/// - The workspace settings in `Cargo.toml`: members, edition 2024, the MSRV,
///   the license, lints (Clippy `all` and `pedantic`, `unsafe_code` denied,
///   `unreachable_pub` warned) and a faster-building `dev` profile. Dependencies
///   stay yours.
/// - `clippy.toml`, `rustfmt.toml` and `.config/nextest.toml`, which requires
///   the pinned nextest version.
/// - `.tool-versions`, so mise installs `cargo-nextest`. Use [`Tools::mise_toml`]
///   to put the tool pins in `mise.toml` instead.
/// - Task definitions in `mise.toml`: `check`, `test`, `test-ci`, `lint`,
///   `fmt`, `fmt-check`, `doc-check`, `framer` and `framer-check`. Custom
///   tasks and other settings in `mise.toml` stay yours. Use
///   [`RustMonorepo::with_just`] for a [`justfile`](framer::just::Justfile) block instead.
/// - `.github/workflows/ci.yml`, running formatting, Clippy, tests, docs and
///   the generated-files check through those tasks.
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
    runner: TaskRunner,
    ci: Option<Ci>,
    dependabot: Option<Dependabot>,
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
                .rust("unreachable_pub", Level::Warn)
                .cargo_group("pedantic", Level::Warn),
            rustfmt: Rustfmt::new(),
            tools: Tools::new(),
            runner: TaskRunner::Mise,
            ci: Some(Ci::new()),
            dependabot: Some(
                Dependabot::new().update(
                    DependabotUpdate::new("cargo")
                        .cooldown_days(7)
                        .commit_message_prefix("chore(deps):")
                        .group("cargo-minor-patch", ["minor", "patch"]),
                ),
            ),
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

    /// Generates a `justfile` block and runs `just` in CI instead of generating
    /// mise tasks. Mise still installs the tools, including `just`.
    pub fn with_just(mut self) -> Self {
        self.runner = TaskRunner::Just;
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

    pub fn dependabot(mut self, dependabot: Dependabot) -> Self {
        self.dependabot = Some(dependabot);
        self
    }

    pub fn without_dependabot(mut self) -> Self {
        self.dependabot = None;
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

    fn tool_versions(&self) -> Tools {
        let tools = match self.runner {
            TaskRunner::Mise => self.tools.clone(),
            TaskRunner::Just => self.tools.clone().with_default_just(),
        };
        self.release
            .dist_config()
            .map(Dist::tool)
            .into_iter()
            .fold(tools, Tools::with_default)
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
}

impl Preset for RustMonorepo {
    fn apply(self, project: &mut Project) {
        project.add(self.rust.toolchain_file());
        project.add(self.cargo_workspace());
        project.add(TomlFile::new("clippy.toml").set("msrv", self.rust.msrv()));
        project.add(self.rustfmt.file());
        project.add(self.nextest_config());
        self.tool_versions().add_to_project(project);
        self.runner.add_tasks(
            project,
            BLOCK,
            self.workspace.extra_manifest_paths(),
            self.release
                .dist_config()
                .into_iter()
                .flat_map(|_| Dist::tasks()),
        );
        if let Some(ci) = &self.ci {
            let workflow = ci.workflow(self.runner);
            let dist_job = self
                .release
                .dist_config()
                .and_then(|dist| dist.ci_job(self.runner));
            project.add(match dist_job {
                Some(job) => workflow.job(job),
                None => workflow,
            });
        }
        if let Some(dependabot) = &self.dependabot {
            project.add(dependabot.clone());
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
