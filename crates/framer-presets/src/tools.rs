use framer::toml_edit::{Item, Table, value};
use framer::{Project, TextFile, TomlKeys};

const JUST: &str = "just";
const NEXTEST: &str = "aqua:nextest-rs/nextest/cargo-nextest";

/// The tools [mise](https://mise.jdx.dev) installs locally and in CI.
#[derive(Clone, Debug)]
pub struct Tools {
    tools: Vec<(String, String)>,
    mise_toml: bool,
}

impl Default for Tools {
    fn default() -> Self {
        Self::new()
    }
}

impl Tools {
    /// `cargo-nextest`, which the generated test tasks run.
    pub fn new() -> Self {
        Self {
            tools: Vec::new(),
            mise_toml: false,
        }
        .nextest("0.9.152")
    }

    /// Generates the `[tools]` table in `mise.toml` instead of `.tool-versions`,
    /// so `mise lock` can lock the tool downloads. Add extra tools with [`Tools::tool`].
    pub fn mise_toml(mut self) -> Self {
        self.mise_toml = true;
        self
    }

    /// Pins `just`. Use [`crate::RustMonorepo::with_just`] to select it as the task runner.
    pub fn just(self, version: &str) -> Self {
        self.tool(JUST, version)
    }

    /// The `cargo-nextest` version, which `.config/nextest.toml` also requires.
    pub fn nextest(self, version: &str) -> Self {
        self.tool(NEXTEST, version)
    }

    /// Adds a tool, such as `actionlint`, or changes the version of one already listed.
    pub fn tool(mut self, name: &str, version: &str) -> Self {
        match self.tools.iter_mut().find(|(existing, _)| existing == name) {
            Some(tool) => version.clone_into(&mut tool.1),
            None => self.tools.push((name.into(), version.into())),
        }
        self
    }

    pub(crate) fn with_default_just(self) -> Self {
        self.with_default((JUST.into(), "1.58.0".into()))
    }

    pub(crate) fn nextest_version(&self) -> &str {
        self.tools
            .iter()
            .find(|(name, _)| name == NEXTEST)
            .map(|(_, version)| version.as_str())
            .expect("Tools::new adds nextest and nothing removes it")
    }

    pub(crate) fn add_to_project(self, project: &mut Project) {
        if self.mise_toml {
            let tools: Table = self
                .tools
                .into_iter()
                .map(|(name, version)| (name, value(version)))
                .collect();
            project.add(TomlKeys::new("mise.toml").set_item("tools", Item::Table(tools)));
        } else {
            let contents: String = self
                .tools
                .iter()
                .flat_map(|(name, version)| [name, " ", version, "\n"])
                .collect();
            project.add(TextFile::new(".tool-versions", contents));
        }
    }

    /// Adds a tool unless the list already has it, so a version set
    /// explicitly wins over a preset's.
    pub(crate) fn with_default(self, (name, version): (String, String)) -> Self {
        if self.tools.iter().any(|(existing, _)| *existing == name) {
            self
        } else {
            self.tool(&name, &version)
        }
    }
}
