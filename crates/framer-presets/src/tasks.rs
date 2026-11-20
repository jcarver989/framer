use framer::just::{Justfile, Recipe};
use framer::toml_edit::{Array, Item, Table, Value, value};
use framer::{Project, TomlKeys};

#[derive(Clone, Copy, Debug)]
pub(crate) enum TaskRunner {
    Mise,
    Just,
}

impl TaskRunner {
    pub(crate) fn command(self, task: &str) -> String {
        match self {
            Self::Mise => format!("mise run {task}"),
            Self::Just => format!("just {task}"),
        }
    }

    pub(crate) fn add_tasks(
        self,
        project: &mut Project,
        block: &str,
        manifests: &[String],
        extra: impl IntoIterator<Item = Task>,
    ) {
        let check = Task::new("check", "Type-check every crate and target")
            .line("cargo check --workspace --all-targets --all-features");
        let (nextest, profile) = match self {
            // Cargo may pick up a separately installed binary from CARGO_HOME/bin.
            Self::Mise => (
                r#""$(mise which cargo-nextest)" nextest"#,
                r#""${usage_profile?}""#,
            ),
            Self::Just => ("cargo nextest", "{{profile}}"),
        };
        let test = Task::new(
            "test",
            "Run tests with nextest, then doctests, which nextest doesn't run",
        )
        .line(format!(
            "{nextest} run --profile {profile} --workspace --all-features --no-tests pass"
        ))
        .line("cargo test --doc --workspace --all-features");
        let test_ci = Task::new(
            "test-ci",
            "Run tests with nextest's CI profile, which writes JUnit results",
        );
        let remaining = common_tasks(manifests).into_iter().chain(extra);

        match self {
            Self::Mise => {
                let mut test = test.table();
                test.insert("usage", value(r#"arg "[profile]" default="default""#));
                let mut test_ci = test_ci.table();
                test_ci.insert("depends", value(Array::from_iter(["test ci"])));
                let config = check
                    .add_to_mise(TomlKeys::new("mise.toml"))
                    .set_item("tasks.test", Item::Table(test))
                    .set_item("tasks.test-ci", Item::Table(test_ci));
                project.add(remaining.fold(config, |config, task| task.add_to_mise(config)));
            }
            Self::Just => {
                let justfile = Justfile::new(block)
                    .setting(r#"shell := ["bash", "-euo", "pipefail", "-c"]"#)
                    .recipe(
                        Recipe::new("default")
                            .doc("List available recipes")
                            .line("@just --list"),
                    )
                    .recipe(check.recipe())
                    .recipe(test.recipe().parameters(r#"profile="default""#))
                    .recipe(test_ci.recipe().dependencies(r#"(test "ci")"#));
                project
                    .add(remaining.fold(justfile, |justfile, task| justfile.recipe(task.recipe())));
            }
        }
    }
}

pub(crate) struct Task {
    name: &'static str,
    description: &'static str,
    lines: Vec<String>,
}

impl Task {
    pub(crate) fn new(name: &'static str, description: &'static str) -> Self {
        Self {
            name,
            description,
            lines: Vec::new(),
        }
    }

    pub(crate) fn line(mut self, line: impl Into<String>) -> Self {
        self.lines.push(line.into());
        self
    }

    fn recipe(self) -> Recipe {
        self.lines
            .into_iter()
            .fold(Recipe::new(self.name).doc(self.description), Recipe::line)
    }

    fn table(self) -> Table {
        let mut table = Table::new();
        table.insert("description", value(self.description));
        match self.lines.as_slice() {
            [] => {}
            [line] => {
                table.insert("run", value(line.as_str()));
            }
            _ => {
                table.insert(
                    "run",
                    value(self.lines.into_iter().map(Value::from).collect::<Array>()),
                );
            }
        }
        table
    }

    fn add_to_mise(self, config: TomlKeys) -> TomlKeys {
        config.set_item(format!("tasks.{}", self.name), Item::Table(self.table()))
    }
}

fn common_tasks(manifests: &[String]) -> [Task; 6] {
    let mut lint = Task::new(
        "lint",
        "Run Clippy, failing on any rustc, Clippy or Cargo warning",
    )
    .line("CARGO_BUILD_WARNINGS=deny cargo clippy --workspace --all-targets --all-features");
    let mut fmt = Task::new("fmt", "Format all code").line("cargo fmt --all");
    let mut fmt_check =
        Task::new("fmt-check", "Check formatting").line("cargo fmt --all -- --check");
    for manifest in manifests {
        lint = lint.line(format!(
            "CARGO_BUILD_WARNINGS=deny cargo clippy --manifest-path {manifest} --target-dir target --all-targets --all-features"
        ));
        fmt = fmt.line(format!("cargo fmt --manifest-path {manifest}"));
        fmt_check = fmt_check.line(format!("cargo fmt --manifest-path {manifest} -- --check"));
    }
    [
        lint,
        fmt,
        fmt_check,
        Task::new("doc-check", "Build docs, failing on any warning")
            .line(r#"RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps --document-private-items"#),
        Task::new("framer", "Regenerate files from .framer/")
            .line("cargo run --quiet --manifest-path .framer/Cargo.toml"),
        Task::new("framer-check", "Check generated files are up to date")
            .line("cargo run --quiet --locked --manifest-path .framer/Cargo.toml -- --check"),
    ]
}
