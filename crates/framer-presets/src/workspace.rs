use framer::rust::{CargoWorkspace, Edition, Lints, Profile};

/// The crates in the workspace and the settings they share in `Cargo.toml`.
///
/// framer owns only these keys. `version`, `[workspace.dependencies]` and
/// everything else stay yours.
#[derive(Clone, Debug)]
pub struct Workspace {
    cargo: CargoWorkspace,
    extra_manifests: Vec<String>,
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

impl Workspace {
    /// Every crate under `crates/`, edition 2024, MIT licensed, with a `dev`
    /// profile that keeps only line tables for your crates and no debug info
    /// for dependencies.
    pub fn new() -> Self {
        Self {
            cargo: CargoWorkspace::new()
                .edition(Edition::E2024)
                .license("MIT")
                .profile(
                    Profile::new("dev")
                        .set("debug", "line-tables-only")
                        .package("*", "debug", false),
                ),
            extra_manifests: Vec::new(),
        }
    }

    /// Replaces the members. Globs such as `crates/ui/*` group crates into folders.
    pub fn members<T: Into<String>>(mut self, members: impl IntoIterator<Item = T>) -> Self {
        self.cargo = self.cargo.members(members);
        self
    }

    /// Paths to keep out of the workspace, such as a vendored crate.
    pub fn exclude<T: Into<String>>(mut self, paths: impl IntoIterator<Item = T>) -> Self {
        self.cargo = self.cargo.exclude(paths);
        self
    }

    /// Cargo manifests outside the workspace, such as a test fixture crate,
    /// that `lint`, `fmt` and `fmt-check` cover too. They build into the
    /// workspace's `target/`.
    pub fn extra_manifests<T: Into<String>>(mut self, paths: impl IntoIterator<Item = T>) -> Self {
        self.extra_manifests
            .extend(paths.into_iter().map(Into::into));
        self
    }

    /// The SPDX license expression, `MIT` by default.
    pub fn license(mut self, license: &str) -> Self {
        self.cargo = self.cargo.license(license);
        self
    }

    pub fn repository(mut self, url: &str) -> Self {
        self.cargo = self.cargo.repository(url);
        self
    }

    /// Adds a build profile, replacing the profile with the same name, such as `dev`.
    pub fn profile(mut self, profile: Profile) -> Self {
        self.cargo = self.cargo.profile(profile);
        self
    }

    pub(crate) fn extra_manifest_paths(&self) -> &[String] {
        &self.extra_manifests
    }

    pub(crate) fn cargo_workspace(&self, msrv: &str, lints: Lints) -> CargoWorkspace {
        self.cargo.clone().rust_version(msrv).lints(lints)
    }
}
