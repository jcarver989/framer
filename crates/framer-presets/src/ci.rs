use framer::github::{self, Job, Step};

const CHECKOUT: &str = "actions/checkout@v7";
const MISE: &str = "jdx/mise-action@v5.2.0";
const RUST_CACHE: &str = "Swatinem/rust-cache@v2";

/// `.github/workflows/ci.yml`: formatting, Clippy, tests, docs and the
/// generated-files check, each running its `just` recipe. Tools come from
/// `.tool-versions` through mise, and Rust from `rust-toolchain.toml`.
///
/// Builds on [`framer::github::Ci`], adding the preset's jobs and the steps
/// every job that compiles shares.
#[derive(Clone, Debug)]
pub struct Ci {
    matrix: Vec<String>,
    setup: Vec<Step>,
    jobs: Vec<CiJob>,
    framer_check: bool,
}

#[derive(Clone, Debug)]
enum CiJob {
    Plain(Job),
    Compiling(CompilingJob),
}

impl Default for Ci {
    fn default() -> Self {
        Self::new()
    }
}

impl Ci {
    /// Tests on `ubuntu-latest`, with the generated-files check.
    pub fn new() -> Self {
        Self {
            matrix: vec!["ubuntu-latest".into()],
            setup: Vec::new(),
            jobs: Vec::new(),
            framer_check: true,
        }
    }

    /// The runners matrix jobs, such as the tests, run on, replacing `ubuntu-latest`.
    pub fn matrix<T: Into<String>>(mut self, runners: impl IntoIterator<Item = T>) -> Self {
        self.matrix = runners.into_iter().map(Into::into).collect();
        self
    }

    /// A step to run before building in every job that compiles, such as
    /// installing system libraries.
    pub fn setup(mut self, step: Step) -> Self {
        self.setup.push(step);
        self
    }

    /// Adds a job, replacing the preset's job with the same id.
    pub fn job(mut self, job: Job) -> Self {
        self.jobs.push(CiJob::Plain(job));
        self
    }

    /// Adds a job that compiles the workspace, so it caches builds and runs
    /// the [`Ci::setup`] steps first, replacing the preset's job with the same id.
    pub fn compiling_job(mut self, job: CompilingJob) -> Self {
        self.jobs.push(CiJob::Compiling(job));
        self
    }

    /// Leaves the generated-files check out of CI, for when CI can't fetch
    /// the `.framer/` crate's dependencies. `just framer-check` still works locally.
    pub fn without_framer_check(mut self) -> Self {
        self.framer_check = false;
        self
    }

    pub(crate) fn workflow(&self) -> github::Ci {
        let test = CompilingJob::new("test", "Test", "test-ci")
            .timeout_minutes(30)
            .runs_on_matrix()
            .step(
                Step::uses("actions/upload-artifact@v6")
                    .name("Upload test results")
                    .condition("always()")
                    .with("name", "test-results-${{ runner.os }}")
                    .with("path", "target/nextest/ci/junit.xml")
                    .with("if-no-files-found", "ignore"),
            );
        let compiling = [
            CompilingJob::new("lint", "Clippy", "lint"),
            test,
            CompilingJob::new("docs", "Docs", "doc-check").timeout_minutes(15),
        ];
        let mut ci = github::Ci::new().matrix(self.matrix.clone()).job(just_job(
            "fmt",
            "Formatting",
            "fmt-check",
            15,
        ));
        ci = compiling.iter().fold(ci, |ci, job| ci.job(self.build(job)));
        ci = if self.framer_check {
            ci.job(just_job("framer", "Generated files", "framer-check", 15))
        } else {
            ci.without_framer_check()
        };
        self.jobs.iter().fold(ci, |ci, job| match job {
            CiJob::Plain(job) => ci.job(job.clone()),
            CiJob::Compiling(job) => ci.job(self.build(job)),
        })
    }

    fn build(&self, job: &CompilingJob) -> Job {
        let mut built = Job::new(&job.id)
            .name(&job.name)
            .timeout_minutes(job.timeout_minutes);
        if job.matrix {
            built = built.runs_on_matrix();
        }
        built.steps_mut().extend(
            [checkout(), install_tools(), rust_cache_saved_on_main()]
                .into_iter()
                .chain(self.setup.iter().cloned())
                .chain(job.before.iter().cloned())
                .chain([Step::run(format!("just {}", job.recipe))])
                .chain(job.after.iter().cloned()),
        );
        built
    }
}

/// A CI job that runs `just <recipe>` after checking out, installing tools,
/// restoring the build cache and running the [`Ci::setup`] steps.
#[derive(Clone, Debug)]
pub struct CompilingJob {
    id: String,
    name: String,
    recipe: String,
    timeout_minutes: u32,
    matrix: bool,
    before: Vec<Step>,
    after: Vec<Step>,
}

impl CompilingJob {
    /// A job on `ubuntu-latest` with a 20-minute timeout.
    pub fn new(id: &str, name: &str, recipe: &str) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            recipe: recipe.into(),
            timeout_minutes: 20,
            matrix: false,
            before: Vec::new(),
            after: Vec::new(),
        }
    }

    pub fn timeout_minutes(mut self, minutes: u32) -> Self {
        self.timeout_minutes = minutes;
        self
    }

    /// Runs the job on every runner in [`Ci::matrix`].
    pub fn runs_on_matrix(mut self) -> Self {
        self.matrix = true;
        self
    }

    /// A step to run after the setup steps and before the recipe, such as
    /// installing a tool only this job needs.
    pub fn before(mut self, step: Step) -> Self {
        self.before.push(step);
        self
    }

    /// A step to run after the recipe.
    pub fn step(mut self, step: Step) -> Self {
        self.after.push(step);
        self
    }
}

pub(crate) fn checkout() -> Step {
    Step::uses(CHECKOUT).name("Checkout repository")
}

pub(crate) fn rust_cache() -> Step {
    Step::uses(RUST_CACHE).name("Cache Rust builds")
}

/// A job that runs `just <recipe>` without compiling the workspace.
pub(crate) fn just_job(id: &str, name: &str, recipe: &str, timeout_minutes: u32) -> Job {
    Job::new(id)
        .name(name)
        .timeout_minutes(timeout_minutes)
        .step(checkout())
        .step(install_tools())
        .step(Step::run(format!("just {recipe}")))
}

fn install_tools() -> Step {
    Step::uses(MISE).name("Install tools")
}

/// Pull requests reuse the cache from `main` but don't write their own.
fn rust_cache_saved_on_main() -> Step {
    rust_cache().with("save-if", "${{ github.ref == 'refs/heads/main' }}")
}
