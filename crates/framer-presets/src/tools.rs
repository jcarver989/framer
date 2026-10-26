const JUST: &str = "just";
const NEXTEST: &str = "aqua:nextest-rs/nextest/cargo-nextest";

/// `.tool-versions`, the tools [mise](https://mise.jdx.dev) installs locally and in CI.
#[derive(Clone, Debug)]
pub struct Tools {
    tools: Vec<(String, String)>,
}

impl Default for Tools {
    fn default() -> Self {
        Self::new()
    }
}

impl Tools {
    /// `just` and `cargo-nextest`, which the generated recipes run.
    pub fn new() -> Self {
        Self { tools: Vec::new() }.just("1.58.0").nextest("0.9.144")
    }

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

    pub(crate) fn nextest_version(&self) -> &str {
        self.tools
            .iter()
            .find(|(name, _)| name == NEXTEST)
            .map(|(_, version)| version.as_str())
            .expect("Tools::new adds nextest and nothing removes it")
    }

    pub(crate) fn file_contents(&self) -> String {
        self.tools
            .iter()
            .flat_map(|(name, version)| [name, " ", version, "\n"])
            .collect()
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
