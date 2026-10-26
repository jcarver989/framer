use crate::ci::just_job;
use framer::TomlFile;
use framer::github::Job;
use framer::just::Recipe;
use framer::rust::Profile;
use framer::toml_edit::{Array, Value};

const CONFIG: &str = "dist-workspace.toml";

/// Binary releases with [dist](https://opensource.axo.dev/cargo-dist/):
/// archives, installers and GitHub Releases, built when a release tag is pushed.
///
/// framer generates `dist-workspace.toml`, the `[profile.dist]` build profile
/// and `just dist-*` recipes, and pins `cargo-dist` in `.tool-versions`. dist
/// itself still generates `.github/workflows/release.yml` from that config: run
/// `just dist-generate` after changing it. CI fails if the two drift apart,
/// unless the workflow is customized with [`Dist::allow_dirty_ci`].
///
/// Crates opt in to binary releases with `[package.metadata.dist] dist = true`.
#[derive(Clone, Debug)]
pub struct Dist {
    version: String,
    targets: Vec<String>,
    installers: Vec<String>,
    homebrew_tap: Option<String>,
    allow_dirty_ci: bool,
    settings: Vec<(String, Value)>,
}

impl Default for Dist {
    fn default() -> Self {
        Self::new()
    }
}

fn array<T: Into<String>>(values: impl IntoIterator<Item = T>) -> Array {
    values
        .into_iter()
        .map(|value| Value::from(value.into()))
        .collect()
}

impl Dist {
    /// Shell installers for Apple Silicon macOS and Linux on ARM and x86.
    pub fn new() -> Self {
        Self {
            version: "0.31.0".into(),
            targets: vec![
                "aarch64-apple-darwin".into(),
                "aarch64-unknown-linux-gnu".into(),
                "x86_64-unknown-linux-gnu".into(),
            ],
            installers: vec!["shell".into()],
            homebrew_tap: None,
            allow_dirty_ci: false,
            settings: Vec::new(),
        }
    }

    /// The dist version, used both by `just dist-*` and in CI.
    pub fn version(mut self, version: &str) -> Self {
        version.clone_into(&mut self.version);
        self
    }

    pub fn targets<T: Into<String>>(mut self, triples: impl IntoIterator<Item = T>) -> Self {
        self.targets = triples.into_iter().map(Into::into).collect();
        self
    }

    pub fn installers<T: Into<String>>(mut self, installers: impl IntoIterator<Item = T>) -> Self {
        self.installers = installers.into_iter().map(Into::into).collect();
        self
    }

    /// Publishes a Homebrew formula to `tap`, such as `me/homebrew-tap`.
    pub fn homebrew_tap(mut self, tap: &str) -> Self {
        self.homebrew_tap = Some(tap.into());
        self
    }

    /// For a hand-customized `release.yml`: dist stops checking it matches the
    /// config, and CI stops running `just dist-check`.
    pub fn allow_dirty_ci(mut self) -> Self {
        self.allow_dirty_ci = true;
        self
    }

    /// Any other `[dist]` setting.
    pub fn set(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.settings.push((key.into(), value.into()));
        self
    }

    pub(crate) fn tool(&self) -> (String, String) {
        ("cargo-dist".into(), self.version.clone())
    }

    pub(crate) fn profile() -> Profile {
        Profile::new("dist").inherits("release").lto("thin")
    }

    pub(crate) fn config(&self) -> TomlFile {
        let mut installers = self.installers.clone();
        if self.homebrew_tap.is_some()
            && !installers.iter().any(|installer| installer == "homebrew")
        {
            installers.push("homebrew".into());
        }
        let mut config = TomlFile::new(CONFIG)
            .set("workspace.members", array(["cargo:."]))
            .set("dist.cargo-dist-version", self.version.as_str())
            .set("dist.ci", "github")
            .set("dist.targets", array(&self.targets))
            .set("dist.installers", array(installers));
        if let Some(tap) = &self.homebrew_tap {
            config = config
                .set("dist.tap", tap.as_str())
                .set("dist.publish-jobs", array(["homebrew"]));
        }
        config = config
            .set("dist.install-path", "CARGO_HOME")
            .set("dist.install-updater", false);
        if self.allow_dirty_ci {
            config = config.set("dist.allow-dirty", array(["ci"]));
        }
        self.settings.iter().fold(config, |config, (key, value)| {
            config.set(format!("dist.{key}"), value.clone())
        })
    }

    pub(crate) fn recipes() -> [Recipe; 3] {
        [
            Recipe::new("dist-plan")
                .doc("Preview what dist will build for the next release")
                .line("dist plan"),
            Recipe::new("dist-generate")
                .doc("Regenerate the release workflow from dist-workspace.toml")
                .line("dist generate"),
            Recipe::new("dist-check")
                .doc("Check the release workflow matches dist-workspace.toml")
                .line("dist generate --check"),
        ]
    }

    pub(crate) fn ci_job(&self) -> Option<Job> {
        (!self.allow_dirty_ci).then(|| just_job("dist", "Release workflow", "dist-check", 15))
    }
}
