use framer::just::{Justfile, Recipe};
use framer::testing::{render, render_into};
use framer::{MemoryFs, Project, Result};
use indoc::indoc;

#[test]
fn renders_settings_and_recipes_into_a_block() -> Result<()> {
    let mut project = Project::new();
    project.add(
        Justfile::new("tools")
            .setting(r#"shell := ["bash", "-c"]"#)
            .recipe(Recipe::new("check").doc("Type-check").line("cargo check"))
            .recipe(
                Recipe::new("test")
                    .parameters(r#"profile="default""#)
                    .line("cargo nextest run --profile {{profile}}"),
            )
            .recipe(Recipe::new("test-ci").dependencies(r#"(test "ci")"#)),
    );

    let fs = render(&project)?;

    assert_eq!(
        fs.get("justfile"),
        Some(indoc! {r#"
            # framer:begin tools
            set shell := ["bash", "-c"]

            # Type-check
            check:
                cargo check

            test profile="default":
                cargo nextest run --profile {{profile}}

            test-ci: (test "ci")
            # framer:end tools
        "#})
    );
    Ok(())
}

#[test]
fn a_recipe_with_the_same_name_replaces_the_earlier_one() -> Result<()> {
    let mut project = Project::new();
    project.add(
        Justfile::new("tools")
            .recipe(Recipe::new("check").line("cargo check"))
            .recipe(Recipe::new("check").line("cargo check --all-features")),
    );

    let fs = render(&project)?;

    let justfile = fs.get("justfile").expect("the justfile was generated");
    assert_eq!(justfile.matches("check:").count(), 1);
    assert!(justfile.contains("    cargo check --all-features\n"));
    Ok(())
}

#[test]
fn recipes_outside_the_block_stay_yours() -> Result<()> {
    let mut project = Project::new();
    project.add(Justfile::new("tools").recipe(Recipe::new("check").line("cargo check")));
    let existing = MemoryFs::new().with_file("justfile", "run:\n    cargo run\n");

    let fs = render_into(&project, existing)?;

    assert!(
        fs.get("justfile")
            .expect("the justfile was generated")
            .starts_with("run:\n    cargo run\n\n# framer:begin tools\n")
    );
    Ok(())
}
