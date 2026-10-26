//! GitHub Actions workflows, plus [`Ci`]: a ready-made Rust CI workflow that
//! also checks the generated files are up to date.

use crate::error::Result;
use crate::output::Output;
use crate::project::Component;
use crate::yaml::Yaml;
use std::path::PathBuf;

fn pairs(entries: &[(String, String)]) -> Option<Yaml> {
    (!entries.is_empty()).then(|| {
        Yaml::Map(
            entries
                .iter()
                .map(|(key, value)| (key.clone(), Yaml::from(value)))
                .collect(),
        )
    })
}

fn upsert(entries: &mut Vec<(String, String)>, key: String, value: String) {
    match entries.iter_mut().find(|(existing, _)| *existing == key) {
        Some(entry) => entry.1 = value,
        None => entries.push((key, value)),
    }
}

/// One step of a job: an action (`uses`) or a shell command (`run`).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Step {
    name: Option<String>,
    id: Option<String>,
    condition: Option<String>,
    uses: Option<String>,
    run: Option<String>,
    with: Vec<(String, String)>,
    env: Vec<(String, String)>,
}

impl Step {
    pub fn uses(action: impl Into<String>) -> Self {
        Self {
            uses: Some(action.into()),
            ..Self::default()
        }
    }

    pub fn run(command: impl Into<String>) -> Self {
        Self {
            run: Some(command.into()),
            ..Self::default()
        }
    }

    pub fn checkout() -> Self {
        Self::uses("actions/checkout@v6").name("Checkout")
    }

    /// Installs the toolchain from `rust-toolchain.toml` and caches build artifacts.
    pub fn rust_toolchain() -> Self {
        Self::uses("actions-rust-lang/setup-rust-toolchain@v1").name("Install Rust")
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// The step's `if:` expression.
    pub fn condition(mut self, expression: impl Into<String>) -> Self {
        self.condition = Some(expression.into());
        self
    }

    pub fn with(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        upsert(&mut self.with, key.into(), value.into());
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        upsert(&mut self.env, key.into(), value.into());
        self
    }

    fn to_yaml(&self) -> Yaml {
        Yaml::map()
            .with_some("name", self.name.as_ref())
            .with_some("id", self.id.as_ref())
            .with_some("if", self.condition.as_ref())
            .with_some("uses", self.uses.as_ref())
            .with_some("with", pairs(&self.with))
            .with_some("env", pairs(&self.env))
            .with_some("run", self.run.as_ref())
    }
}

/// Where a job runs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RunsOn {
    Runner(String),
    /// Every runner in the workflow's operating system matrix.
    Matrix,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Job {
    id: String,
    name: Option<String>,
    runs_on: RunsOn,
    permissions: Vec<(String, String)>,
    timeout_minutes: Option<u32>,
    needs: Vec<String>,
    env: Vec<(String, String)>,
    steps: Vec<Step>,
}

impl Job {
    /// An empty job on `ubuntu-latest`.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: None,
            runs_on: RunsOn::Runner("ubuntu-latest".into()),
            permissions: Vec::new(),
            timeout_minutes: None,
            needs: Vec::new(),
            env: Vec::new(),
            steps: Vec::new(),
        }
    }

    /// Grants this job's token a permission, such as `contents: write`,
    /// replacing the workflow's permissions for this job.
    pub fn permission(mut self, scope: impl Into<String>, access: impl Into<String>) -> Self {
        upsert(&mut self.permissions, scope.into(), access.into());
        self
    }

    /// A job that has checked out the repository and installed Rust.
    pub fn rust(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self::new(id)
            .name(name)
            .timeout_minutes(30)
            .step(Step::checkout())
            .step(Step::rust_toolchain())
    }

    /// A Rust job that runs a single command.
    pub fn command(id: impl Into<String>, command: impl Into<String>) -> Self {
        let id = id.into();
        Self::rust(id.clone(), id).step(Step::run(command))
    }

    pub fn fmt() -> Self {
        Self::rust("fmt", "Formatting")
            .timeout_minutes(15)
            .step(Step::run("cargo fmt --all --check").name("Check formatting"))
    }

    pub fn clippy() -> Self {
        Self::rust("clippy", "Clippy").step(
            Step::run("cargo clippy --workspace --all-targets --all-features -- -D warnings")
                .name("Run Clippy"),
        )
    }

    /// `cargo test` on every operating system in the matrix.
    pub fn test() -> Self {
        Self::rust("test", "Test")
            .runs_on_matrix()
            .step(Step::run("cargo test --workspace --all-features").name("Run tests"))
    }

    /// `cargo nextest` on every operating system in the matrix.
    pub fn nextest() -> Self {
        Self::rust("test", "Test")
            .runs_on_matrix()
            .step(Step::uses("taiki-e/install-action@nextest").name("Install nextest"))
            .step(Step::run("cargo nextest run --workspace --all-features").name("Run tests"))
    }

    pub fn docs() -> Self {
        Self::rust("docs", "Docs").timeout_minutes(15).step(
            Step::run("cargo doc --workspace --all-features --no-deps")
                .name("Check documentation")
                .env("RUSTDOCFLAGS", "-D warnings"),
        )
    }

    /// Fails when someone edited a generated file by hand or forgot to run `cargo framer`.
    pub fn framer_check() -> Self {
        Self::rust("framer", "Generated files")
            .timeout_minutes(15)
            .step(
                Step::run("cargo run --locked --manifest-path .framer/Cargo.toml -- --check")
                    .name("Check generated files are up to date"),
            )
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn runs_on(mut self, runner: impl Into<String>) -> Self {
        self.runs_on = RunsOn::Runner(runner.into());
        self
    }

    pub fn runs_on_matrix(mut self) -> Self {
        self.runs_on = RunsOn::Matrix;
        self
    }

    pub fn timeout_minutes(mut self, minutes: u32) -> Self {
        self.timeout_minutes = Some(minutes);
        self
    }

    pub fn needs(mut self, job_id: impl Into<String>) -> Self {
        self.needs.push(job_id.into());
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        upsert(&mut self.env, key.into(), value.into());
        self
    }

    pub fn step(mut self, step: Step) -> Self {
        self.steps.push(step);
        self
    }

    /// Inserts `step` right after the step with the given name.
    ///
    /// # Panics
    ///
    /// When the job has no step with that name. Misconfiguring a job is a bug
    /// in the project definition, so it fails loudly instead of being ignored.
    pub fn step_after(mut self, name: &str, step: Step) -> Self {
        let index = self
            .steps
            .iter()
            .position(|existing| existing.name.as_deref() == Some(name))
            .unwrap_or_else(|| panic!("job `{}` has no step named `{name}`", self.id));
        self.steps.insert(index + 1, step);
        self
    }

    pub fn steps_mut(&mut self) -> &mut Vec<Step> {
        &mut self.steps
    }

    fn to_yaml(&self, matrix: &[String]) -> Yaml {
        let mut job = Yaml::map().with_some("name", self.name.as_ref()).with_some(
            "needs",
            (!self.needs.is_empty()).then(|| self.needs.clone()),
        );
        job = match (&self.runs_on, matrix) {
            (RunsOn::Runner(runner), _) | (RunsOn::Matrix, [runner]) => job.with("runs-on", runner),
            (RunsOn::Matrix, []) => job.with("runs-on", "ubuntu-latest"),
            (RunsOn::Matrix, runners) => job
                .with(
                    "strategy",
                    Yaml::map()
                        .with("fail-fast", false)
                        .with("matrix", Yaml::map().with("os", runners.to_vec())),
                )
                .with("runs-on", "${{ matrix.os }}"),
        };
        job.with_some("permissions", pairs(&self.permissions))
            .with_some("timeout-minutes", self.timeout_minutes)
            .with_some("env", pairs(&self.env))
            .with(
                "steps",
                Yaml::Seq(self.steps.iter().map(Step::to_yaml).collect()),
            )
    }
}

/// When a workflow runs.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Triggers {
    push_branches: Option<Vec<String>>,
    pull_request: bool,
    workflow_dispatch: bool,
    schedules: Vec<String>,
}

impl Triggers {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push<T: Into<String>>(mut self, branches: impl IntoIterator<Item = T>) -> Self {
        self.push_branches = Some(branches.into_iter().map(Into::into).collect());
        self
    }

    pub fn pull_request(mut self) -> Self {
        self.pull_request = true;
        self
    }

    pub fn workflow_dispatch(mut self) -> Self {
        self.workflow_dispatch = true;
        self
    }

    /// Runs on a cron schedule, such as `0 6 * * 1`.
    pub fn schedule(mut self, cron: impl Into<String>) -> Self {
        self.schedules.push(cron.into());
        self
    }

    fn to_yaml(&self) -> Yaml {
        let mut on = Yaml::map();
        if self.pull_request {
            on = on.with("pull_request", Yaml::Null);
        }
        if let Some(branches) = &self.push_branches {
            on = on.with("push", Yaml::map().with("branches", branches.clone()));
        }
        if self.workflow_dispatch {
            on = on.with("workflow_dispatch", Yaml::Null);
        }
        if !self.schedules.is_empty() {
            let crons = self
                .schedules
                .iter()
                .map(|cron| Yaml::map().with("cron", cron))
                .collect();
            on = on.with("schedule", Yaml::Seq(crons));
        }
        on
    }
}

/// A workflow file in `.github/workflows/`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Workflow {
    file: String,
    name: String,
    triggers: Triggers,
    concurrency: Option<(String, bool)>,
    env: Vec<(String, String)>,
    permissions: Vec<(String, String)>,
    matrix: Vec<String>,
    jobs: Vec<Job>,
}

impl Workflow {
    /// A workflow written to `.github/workflows/<file>.yml`.
    pub fn new(file: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            name: name.into(),
            triggers: Triggers::default(),
            concurrency: None,
            env: Vec::new(),
            permissions: Vec::new(),
            matrix: Vec::new(),
            jobs: Vec::new(),
        }
    }

    pub fn path(&self) -> PathBuf {
        PathBuf::from(format!(".github/workflows/{}.yml", self.file))
    }

    /// Runs at most one workflow run per `group` at a time, optionally
    /// cancelling the one in progress.
    pub fn concurrency(mut self, group: impl Into<String>, cancel_in_progress: bool) -> Self {
        self.concurrency = Some((group.into(), cancel_in_progress));
        self
    }

    pub fn on(mut self, triggers: Triggers) -> Self {
        self.triggers = triggers;
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        upsert(&mut self.env, key.into(), value.into());
        self
    }

    /// Grants the workflow's token a permission, such as `contents: read`.
    pub fn permission(mut self, scope: impl Into<String>, access: impl Into<String>) -> Self {
        upsert(&mut self.permissions, scope.into(), access.into());
        self
    }

    /// The runners that jobs using [`RunsOn::Matrix`] run on.
    pub fn matrix<T: Into<String>>(mut self, runners: impl IntoIterator<Item = T>) -> Self {
        self.matrix = runners.into_iter().map(Into::into).collect();
        self
    }

    /// Adds a job, replacing any job with the same id.
    pub fn job(mut self, job: Job) -> Self {
        match self.jobs.iter_mut().find(|existing| existing.id == job.id) {
            Some(existing) => *existing = job,
            None => self.jobs.push(job),
        }
        self
    }

    pub fn job_mut(&mut self, id: &str) -> Option<&mut Job> {
        self.jobs.iter_mut().find(|job| job.id == id)
    }

    pub fn remove_job(&mut self, id: &str) -> Option<Job> {
        let index = self.jobs.iter().position(|job| job.id == id)?;
        Some(self.jobs.remove(index))
    }

    pub fn to_yaml(&self) -> Yaml {
        let jobs = self
            .jobs
            .iter()
            .map(|job| (job.id.clone(), job.to_yaml(&self.matrix)))
            .collect();
        Yaml::map()
            .with("name", &self.name)
            .with("on", self.triggers.to_yaml())
            .with_some(
                "concurrency",
                self.concurrency.as_ref().map(|(group, cancel)| {
                    Yaml::map()
                        .with("group", group)
                        .with("cancel-in-progress", *cancel)
                }),
            )
            .with_some("env", pairs(&self.env))
            .with_some("permissions", pairs(&self.permissions))
            .with("jobs", Yaml::Map(jobs))
    }
}

impl Component for Workflow {
    fn synthesize(&self, output: &mut Output) -> Result<()> {
        output.managed(self.path(), self.to_yaml().to_yaml_string())
    }
}

/// `.github/workflows/ci.yml`: runs on pushes to `main` and on pull requests,
/// and always checks that generated files are up to date.
///
/// ```
/// use framer::github::{Ci, Job};
///
/// let ci = Ci::new()
///     .matrix(["ubuntu-latest", "macos-latest"])
///     .job(Job::fmt())
///     .job(Job::clippy())
///     .job(Job::test());
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ci {
    workflow: Workflow,
    framer_check: bool,
}

impl Default for Ci {
    fn default() -> Self {
        Self::new()
    }
}

impl Ci {
    pub fn new() -> Self {
        let workflow = Workflow::new("ci", "CI")
            .on(Triggers::new().pull_request().push(["main"]))
            .env("CARGO_INCREMENTAL", "0")
            .permission("contents", "read");
        Self {
            workflow,
            framer_check: true,
        }
    }

    /// The runners that matrix jobs, such as [`Job::test`], run on.
    pub fn matrix<T: Into<String>>(mut self, runners: impl IntoIterator<Item = T>) -> Self {
        self.workflow = self.workflow.matrix(runners);
        self
    }

    pub fn branches<T: Into<String>>(mut self, branches: impl IntoIterator<Item = T>) -> Self {
        self.workflow = self
            .workflow
            .on(Triggers::new().pull_request().push(branches));
        self
    }

    pub fn job(mut self, job: Job) -> Self {
        self.workflow = self.workflow.job(job);
        self
    }

    pub fn job_mut(&mut self, id: &str) -> Option<&mut Job> {
        self.workflow.job_mut(id)
    }

    /// Leaves out the job that checks generated files are up to date.
    pub fn without_framer_check(mut self) -> Self {
        self.framer_check = false;
        self
    }

    /// The underlying workflow, for anything else.
    pub fn workflow_mut(&mut self) -> &mut Workflow {
        &mut self.workflow
    }
}

impl Component for Ci {
    fn synthesize(&self, output: &mut Output) -> Result<()> {
        let mut workflow = self.workflow.clone();
        if self.framer_check && workflow.job_mut("framer").is_none() {
            workflow = workflow.job(Job::framer_check());
        }
        workflow.synthesize(output)
    }
}
