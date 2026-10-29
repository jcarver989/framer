use crate::ci::{checkout, rust_cache};
use framer::github::{Job, Step, Triggers, Workflow};
use framer::toml_edit::Value;
use framer::{Project, TomlKeys};

const ACTION: &str = "release-plz/action@v0.5.142";
const APP_TOKEN_ACTION: &str = "actions/create-github-app-token@v3";
const CRATES_IO_AUTH_ACTION: &str = "rust-lang/crates-io-auth-action@v1";
const CONFIG: &str = "release-plz.toml";

/// Release PRs and tags with [release-plz](https://release-plz.dev).
///
/// framer owns the `[workspace]` settings in `release-plz.toml`, and leaves the
/// `[[package]]` entries that choose what each crate releases to you. It also
/// generates `.github/workflows/release-plz.yml`, which opens or updates a
/// release PR on every push to `main` and tags releases once it's merged.
///
/// By default the workflow uses a GitHub App token, from the
/// `RELEASE_AUTOMATION_APP_ID` and `RELEASE_AUTOMATION_APP_PRIVATE_KEY` secrets,
/// because tags pushed with the default `GITHUB_TOKEN` don't start other
/// workflows, such as the release workflow `dist` generates.
#[derive(Clone, Debug)]
pub struct ReleasePlz {
    settings: Vec<(String, Value)>,
    setup: Vec<Step>,
    crates_io: bool,
    app_token_secrets: Option<(String, String)>,
}

impl Default for ReleasePlz {
    fn default() -> Self {
        Self::new()
    }
}

impl ReleasePlz {
    pub fn new() -> Self {
        Self {
            settings: Vec::new(),
            setup: Vec::new(),
            crates_io: false,
            app_token_secrets: Some((
                "RELEASE_AUTOMATION_APP_ID".into(),
                "RELEASE_AUTOMATION_APP_PRIVATE_KEY".into(),
            )),
        }
        .set("release_always", false)
        .set("git_release_enable", false)
        .set("git_tag_name", "{{ package }}-v{{ version }}")
        .set("pr_branch_prefix", "release-plz/")
        .set("pr_name", "chore: release")
        .set("dependencies_update", true)
    }

    /// Any other `[workspace]` setting, replacing the preset's value if it has one.
    pub fn set(mut self, key: &str, value: impl Into<Value>) -> Self {
        let value = value.into();
        match self
            .settings
            .iter_mut()
            .find(|(existing, _)| existing == key)
        {
            Some(setting) => setting.1 = value,
            None => self.settings.push((key.into(), value)),
        }
        self
    }

    /// A step to run before release-plz builds the crates in both jobs, such
    /// as installing system libraries that publishing and semver checks need.
    pub fn setup(mut self, step: Step) -> Self {
        self.setup.push(step);
        self
    }

    /// Lets the workflow publish crates through crates.io trusted publishing,
    /// so no registry token is stored. Each crate needs a trusted publisher
    /// for `release-plz.yml` on crates.io, which only exists after its first
    /// publish, so publish new crates once by hand. Choose which crates
    /// publish with `publish = true` in their `[[package]]` entries.
    pub fn publish_to_crates_io(mut self) -> Self {
        self.crates_io = true;
        self
    }

    /// The secrets holding the GitHub App's id and private key.
    pub fn app_token_secrets(mut self, app_id: &str, private_key: &str) -> Self {
        self.app_token_secrets = Some((app_id.into(), private_key.into()));
        self
    }

    /// Uses the workflow's own `GITHUB_TOKEN` instead of a GitHub App. Tags it
    /// pushes won't start other workflows.
    pub fn github_token(mut self) -> Self {
        self.app_token_secrets = None;
        self
    }

    pub(crate) fn apply(&self, project: &mut Project) {
        let config = self
            .settings
            .iter()
            .fold(TomlKeys::new(CONFIG), |config, (key, value)| {
                config.set(format!("workspace.{key}"), value.clone())
            });
        project.add(config);
        project.add(self.workflow());
    }

    fn workflow(&self) -> Workflow {
        let mut release = self
            .job("release-plz-release", "Release-plz release", "release")
            .permission("contents", "write")
            .permission("pull-requests", "read");
        if self.crates_io {
            release = release.permission("id-token", "write");
        }
        let pr = self
            .job("release-plz-pr", "Release-plz PR", "release-pr")
            .needs("release-plz-release")
            .permission("contents", "write")
            .permission("pull-requests", "write");
        Workflow::new("release-plz", "release-plz")
            .on(Triggers::new().push(["main"]))
            .concurrency("release-plz-${{ github.ref }}", false)
            .job(release)
            .job(pr)
    }

    fn job(&self, id: &str, name: &str, command: &str) -> Job {
        let token = match self.app_token_secrets {
            Some(_) => "${{ steps.app-token.outputs.token }}",
            None => "${{ secrets.GITHUB_TOKEN }}",
        };
        let mut job = Job::new(id).name(name);
        if let Some((app_id, private_key)) = &self.app_token_secrets {
            job = job.step(
                Step::uses(APP_TOKEN_ACTION)
                    .name("Generate release automation app token")
                    .id("app-token")
                    .with("app-id", format!("${{{{ secrets.{app_id} }}}}"))
                    .with("private-key", format!("${{{{ secrets.{private_key} }}}}"))
                    .with("owner", "${{ github.repository_owner }}")
                    .with("repositories", "${{ github.event.repository.name }}"),
            );
        }
        let mut release = Step::uses(ACTION)
            .name(format!("Run release-plz {command}"))
            .with("command", command)
            .env("GITHUB_TOKEN", token);
        job = job
            .step(
                checkout()
                    .with("fetch-depth", "0")
                    .with("persist-credentials", "false")
                    .with("token", token),
            )
            .step(rust_cache());
        job = self.setup.iter().cloned().fold(job, Job::step);
        if self.crates_io && command == "release" {
            job = job.step(
                Step::uses(CRATES_IO_AUTH_ACTION)
                    .name("Authenticate with crates.io")
                    .id("crates-io-auth"),
            );
            release = release.env(
                "CARGO_REGISTRY_TOKEN",
                "${{ steps.crates-io-auth.outputs.token }}",
            );
        }
        job.step(release)
    }
}
