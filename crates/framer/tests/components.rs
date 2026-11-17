use framer::github::{Dependabot, DependabotUpdate, Job, Step, Triggers, Workflow};
use framer::rust::{CargoWorkspace, Edition, Level, Lints, Profile};
use framer::testing::{render, render_into};
use framer::{MemoryFs, Mode, Project, TextFile, TomlKeys};
use indoc::indoc;

#[test]
fn workspace_package_settings_framer_does_not_set_stay_yours() {
    let mut project = Project::new();
    project.add(CargoWorkspace::new().edition(Edition::E2024));
    let existing = MemoryFs::new().with_file(
        "Cargo.toml",
        "[workspace]\nmembers = [\"crates/*\"]\n\n[workspace.package]\nversion = \"0.3.0\"\nedition = \"2021\"\n",
    );

    let mut fs = render_into(&project, existing).unwrap();

    let cargo = fs.get("Cargo.toml").unwrap();
    assert!(cargo.contains("[workspace.package]\nversion = \"0.3.0\"\nedition = \"2024\"\n"));

    // Dropping the edition removes only the key framer owned.
    let mut without_edition = Project::new();
    without_edition.add(CargoWorkspace::new());
    without_edition.synth_with(&mut fs, Mode::Write).unwrap();

    assert!(
        fs.get("Cargo.toml")
            .unwrap()
            .contains("[workspace.package]\nversion = \"0.3.0\"\n")
    );
}

#[test]
fn profiles_can_have_per_package_settings() {
    let mut project = Project::new();
    project.add(
        CargoWorkspace::new().profile(
            Profile::new("dev")
                .set("debug", "line-tables-only")
                .package("*", "debug", false),
        ),
    );

    let fs = render(&project).unwrap();

    assert!(fs.get("Cargo.toml").unwrap().ends_with(indoc! {r#"
        [profile.dev]
        debug = "line-tables-only"

        [profile.dev.package."*"]
        debug = false
    "#}));
}

#[test]
fn cargo_lints_go_after_rustc_and_clippy_lints() {
    let mut project = Project::new();
    project.add(
        CargoWorkspace::new().lints(
            Lints::new()
                .cargo_group("pedantic", Level::Warn)
                .cargo("unused_dependencies", Level::Deny)
                .clippy_group("pedantic", Level::Warn)
                .rust("unsafe_code", Level::Deny),
        ),
    );

    let fs = render(&project).unwrap();

    assert!(fs.get("Cargo.toml").unwrap().ends_with(indoc! {r#"
        [workspace.lints.rust]
        unsafe_code = "deny"

        [workspace.lints.clippy]
        pedantic = { level = "warn", priority = -1 }

        [workspace.lints.cargo]
        pedantic = { level = "warn", priority = -1 }
        unused_dependencies = "deny"
    "#}));
}

#[test]
fn dependabot_updates_each_ecosystem_on_its_schedule() {
    let mut project = Project::new();
    project.add(
        Dependabot::new()
            .update(
                DependabotUpdate::new("cargo")
                    .cooldown_days(7)
                    .commit_message_prefix("chore(deps):")
                    .group("cargo-minor-patch", ["minor", "patch"]),
            )
            .update(
                DependabotUpdate::new("github-actions")
                    .directory("/.github/actions/setup")
                    .interval("daily"),
            ),
    );

    let fs = render(&project).unwrap();

    assert!(
        fs.get(".github/dependabot.yml")
            .unwrap()
            .ends_with(indoc! {r#"
        version: 2
        updates:
          - package-ecosystem: cargo
            directory: /
            schedule:
              interval: weekly
            cooldown:
              default-days: 7
            commit-message:
              prefix: "chore(deps):"
            groups:
              cargo-minor-patch:
                update-types: [minor, patch]
                patterns: ["*"]
          - package-ecosystem: github-actions
            directory: /.github/actions/setup
            schedule:
              interval: daily
    "#})
    );
}

#[test]
fn a_dependabot_update_replaces_the_one_for_the_same_ecosystem_and_directory() {
    let mut project = Project::new();
    project.add(
        Dependabot::new()
            .update(DependabotUpdate::new("cargo"))
            .update(DependabotUpdate::new("cargo").interval("daily")),
    );

    let fs = render(&project).unwrap();

    let config = fs.get(".github/dependabot.yml").unwrap();
    assert_eq!(config.matches("package-ecosystem").count(), 1);
    assert!(config.contains("interval: daily\n"));
}

#[test]
fn toml_keys_leave_the_rest_of_the_file_alone() {
    let mut project = Project::new();
    project.add(
        TomlKeys::new("release-plz.toml")
            .set("workspace.pr_name", "chore: release")
            .set("workspace.dependencies_update", true),
    );
    let existing = MemoryFs::new().with_file(
        "release-plz.toml",
        "[workspace]\nsemver_check = false\n\n[[package]]\nname = \"app\"\n",
    );

    let fs = render_into(&project, existing).unwrap();

    assert_eq!(
        fs.get("release-plz.toml").unwrap(),
        indoc! {r#"
            [workspace]
            semver_check = false
            pr_name = "chore: release"
            dependencies_update = true

            [[package]]
            name = "app"
        "#}
    );
}

#[test]
fn workflows_support_concurrency_and_job_permissions() {
    let mut project = Project::new();
    project.add(
        Workflow::new("release", "release")
            .on(Triggers::new().push(["main"]))
            .concurrency("release-${{ github.ref }}", false)
            .job(
                Job::new("tag")
                    .permission("contents", "write")
                    .step(Step::run("echo tag")),
            )
            .job(Job::new("pr").needs("tag").step(Step::run("echo pr"))),
    );

    let fs = render(&project).unwrap();

    assert!(
        fs.get(".github/workflows/release.yml")
            .unwrap()
            .ends_with(indoc! {r"
        name: release
        on:
          push:
            branches: [main]
        concurrency:
          group: release-${{ github.ref }}
          cancel-in-progress: false
        jobs:
          tag:
            runs-on: ubuntu-latest
            permissions:
              contents: write
            steps:
              - run: echo tag
          pr:
            needs: [tag]
            runs-on: ubuntu-latest
            steps:
              - run: echo pr
    "})
    );
}

#[test]
fn tool_versions_files_get_a_header() {
    let mut project = Project::new();
    project.add(TextFile::new(".tool-versions", "just 1.58.0\n"));

    let fs = render(&project).unwrap();

    assert!(
        fs.get(".tool-versions")
            .unwrap()
            .starts_with("# Generated by framer.")
    );
}
