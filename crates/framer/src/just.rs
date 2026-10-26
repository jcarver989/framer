//! [`Justfile`]: recipes framer keeps in a marked block of `justfile`, so presets
//! can add recipes that repositories then extend or replace by name.

use crate::error::Result;
use crate::output::Output;
use crate::project::Component;

const JUSTFILE: &str = "justfile";

/// One recipe: its name, parameters and dependencies, then its command lines.
///
/// ```
/// use framer::just::Recipe;
///
/// let test = Recipe::new("test")
///     .doc("Run every test")
///     .parameters(r#"profile="default""#)
///     .line("cargo nextest run --profile {{profile}} --workspace");
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Recipe {
    name: String,
    doc: Option<String>,
    parameters: Option<String>,
    dependencies: Option<String>,
    lines: Vec<String>,
}

impl Recipe {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            doc: None,
            parameters: None,
            dependencies: None,
            lines: Vec::new(),
        }
    }

    /// The comment above the recipe, which `just --list` shows.
    pub fn doc(mut self, doc: impl Into<String>) -> Self {
        self.doc = Some(doc.into());
        self
    }

    /// Parameters as written after the name, such as `profile="default"`.
    pub fn parameters(mut self, parameters: impl Into<String>) -> Self {
        self.parameters = Some(parameters.into());
        self
    }

    /// Recipes to run first, as written after the colon, such as `(test "ci")`.
    pub fn dependencies(mut self, dependencies: impl Into<String>) -> Self {
        self.dependencies = Some(dependencies.into());
        self
    }

    /// Adds a command line.
    pub fn line(mut self, line: impl Into<String>) -> Self {
        self.lines.push(line.into());
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// The command lines, for changing a recipe a preset set up.
    pub fn lines_mut(&mut self) -> &mut Vec<String> {
        &mut self.lines
    }

    fn render(&self) -> String {
        let suffixed = |part: &Option<String>| {
            part.as_ref()
                .map(|part| format!(" {part}"))
                .unwrap_or_default()
        };
        let doc = self
            .doc
            .as_ref()
            .map(|doc| format!("# {doc}\n"))
            .unwrap_or_default();
        let header = format!(
            "{}{}:{}\n",
            self.name,
            suffixed(&self.parameters),
            suffixed(&self.dependencies)
        );
        let lines: String = self
            .lines
            .iter()
            .flat_map(|line| ["    ", line, "\n"])
            .collect();
        format!("{doc}{header}{lines}")
    }
}

/// Settings and recipes in a marked block of `justfile`. Recipes outside the
/// block stay the repository's own.
///
/// ```
/// use framer::just::{Justfile, Recipe};
///
/// let justfile = Justfile::new("tools")
///     .setting(r#"shell := ["bash", "-euo", "pipefail", "-c"]"#)
///     .recipe(Recipe::new("check").line("cargo check --workspace"));
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Justfile {
    block: String,
    settings: Vec<String>,
    recipes: Vec<Recipe>,
}

impl Justfile {
    /// The block `block` of `justfile`.
    pub fn new(block: impl Into<String>) -> Self {
        Self {
            block: block.into(),
            settings: Vec::new(),
            recipes: Vec::new(),
        }
    }

    /// A `set` line, such as `shell := ["bash", "-c"]`.
    pub fn setting(mut self, setting: impl Into<String>) -> Self {
        self.settings.push(setting.into());
        self
    }

    /// Adds a recipe, replacing any recipe with the same name.
    pub fn recipe(mut self, recipe: Recipe) -> Self {
        match self
            .recipes
            .iter_mut()
            .find(|existing| existing.name == recipe.name)
        {
            Some(existing) => *existing = recipe,
            None => self.recipes.push(recipe),
        }
        self
    }

    pub fn recipe_mut(&mut self, name: &str) -> Option<&mut Recipe> {
        self.recipes.iter_mut().find(|recipe| recipe.name == name)
    }

    /// Removes a recipe, so the repository can define its own outside the block.
    pub fn remove_recipe(&mut self, name: &str) -> Option<Recipe> {
        let index = self.recipes.iter().position(|recipe| recipe.name == name)?;
        Some(self.recipes.remove(index))
    }

    fn render(&self) -> String {
        let settings: String = self
            .settings
            .iter()
            .flat_map(|setting| ["set ", setting, "\n"])
            .collect();
        let sections: Vec<String> = (!settings.is_empty())
            .then_some(settings)
            .into_iter()
            .chain(self.recipes.iter().map(Recipe::render))
            .collect();
        sections.join("\n")
    }
}

impl Component for Justfile {
    fn synthesize(&self, output: &mut Output) -> Result<()> {
        output.block(JUSTFILE, &self.block, self.render())
    }
}
