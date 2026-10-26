use framer::github::Step;
use framer::testing::{render, render_into, render_preset};
use framer::{MemoryFs, Mode, Project, Result};
use framer_presets::{Dist, ReleasePlz, RustMonorepo};
use indoc::indoc;

#[test]
fn generates_the_whole_setup() -> Result<()> {
    let fs = render_preset(RustMonorepo::new())?;

    let paths: Vec<_> = fs.paths().map(|path| path.display().to_string()).collect();
    assert_eq!(
        paths,
        [
            ".config/nextest.toml",
            ".framer/manifest.json",
            ".github/workflows/ci.yml",
            ".gitignore",
            ".tool-versions",
            "AGENTS.md",
            "Cargo.toml",
            "clippy.toml",
            "justfile",
            "rust-toolchain.toml",
            "rustfmt.toml",
        ]
    );
    assert!(
        file(&fs, "rust-toolchain.toml")
            .contains("channel = \"1.98.0\"\ncomponents = [\"clippy\", \"rustfmt\"]\n")
    );
    assert!(file(&fs, "clippy.toml").ends_with("msrv = \"1.98\"\n"));
    assert!(
        file(&fs, ".tool-versions")
            .ends_with("just 1.58.0\naqua:nextest-rs/nextest/cargo-nextest 0.9.144\n")
    );
    assert!(file(&fs, "AGENTS.md").contains("### Testing"));
    Ok(())
}

#[test]
fn a_second_run_changes_nothing() -> Result<()> {
    let project = project(full_preset());
    let mut fs = render(&project)?;

    let report = project.synth_with(&mut fs, Mode::Check)?;

    assert!(report.is_clean(), "{}", report.diff());
    Ok(())
}

#[test]
fn keeps_what_the_repository_owns() -> Result<()> {
    let existing = MemoryFs::new()
        .with_file(
            "Cargo.toml",
            "[workspace.package]\nversion = \"0.3.0\"\n\n[workspace.dependencies]\nserde = \"1\"\n",
        )
        .with_file("justfile", "run:\n    cargo run -p app\n")
        .with_file(
            "release-plz.toml",
            "[[package]]\nname = \"app\"\nrelease = true\n",
        );

    let fs = render_into(&project(full_preset()), existing)?;

    let cargo = file(&fs, "Cargo.toml");
    assert!(cargo.contains("[workspace.package]\nversion = \"0.3.0\"\nedition = \"2024\"\n"));
    assert!(cargo.contains("[workspace.dependencies]\nserde = \"1\"\n"));
    assert!(
        file(&fs, "justfile")
            .starts_with("run:\n    cargo run -p app\n\n# framer:begin rust-monorepo\n")
    );
    assert!(
        file(&fs, "release-plz.toml").contains("[[package]]\nname = \"app\"\nrelease = true\n")
    );
    Ok(())
}

#[test]
fn workspace_lints_and_profiles() -> Result<()> {
    let fs = render_preset(RustMonorepo::new())?;

    assert!(file(&fs, "Cargo.toml").ends_with(indoc! {r#"
        [workspace.lints.rust]
        unsafe_code = "deny"
        unreachable_pub = "warn"

        [workspace.lints.clippy]
        all = { level = "warn", priority = -1 }
        pedantic = { level = "warn", priority = -1 }
        missing_errors_doc = "allow"
        missing_panics_doc = "allow"
        must_use_candidate = "allow"
        return_self_not_must_use = "allow"

        [profile.dev]
        debug = "line-tables-only"

        [profile.dev.package."*"]
        debug = false
    "#}));
    Ok(())
}

#[test]
fn ci_runs_the_same_recipes_as_local_development() -> Result<()> {
    let fs = render_preset(RustMonorepo::new())?;

    let ci = file(&fs, ".github/workflows/ci.yml");
    for recipe in ["fmt-check", "lint", "test-ci", "doc-check", "framer-check"] {
        assert!(ci.contains(&format!("run: just {recipe}\n")), "{recipe}");
        assert!(
            file(&fs, "justfile").contains(&format!("\n{recipe}:")),
            "{recipe}"
        );
    }
    assert!(ci.contains("uses: jdx/mise-action@v5.2.0"));
    assert!(!ci.contains("dist"));
    Ok(())
}

#[test]
fn setup_steps_run_before_every_job_that_compiles() -> Result<()> {
    let preset = RustMonorepo::new().ci_setup(
        Step::run("sudo apt-get install --yes libfontconfig1-dev").name("Install system libraries"),
    );

    let fs = render_preset(preset)?;

    let ci = file(&fs, ".github/workflows/ci.yml");
    assert_eq!(
        ci.matches("Install system libraries").count(),
        3,
        "lint, test and docs"
    );
    Ok(())
}

#[test]
fn tests_run_on_every_ci_runner() -> Result<()> {
    let fs = render_preset(RustMonorepo::new().ci_runners(["ubuntu-latest", "macos-latest"]))?;

    let ci = file(&fs, ".github/workflows/ci.yml");
    assert!(
        ci.contains("        os: [ubuntu-latest, macos-latest]\n    runs-on: ${{ matrix.os }}\n")
    );
    Ok(())
}

#[test]
fn msrv_toolchain_and_tools_are_configurable() -> Result<()> {
    let preset = RustMonorepo::new()
        .msrv("1.97")
        .toolchain("1.99.1")
        .targets(["wasm32-unknown-unknown"])
        .tool("just", "1.60.0")
        .tool("actionlint", "1.7.12");

    let fs = render_preset(preset)?;

    assert!(file(&fs, "rust-toolchain.toml").contains("channel = \"1.99.1\""));
    assert!(file(&fs, "rust-toolchain.toml").contains("targets = [\"wasm32-unknown-unknown\"]"));
    assert!(file(&fs, "Cargo.toml").contains("rust-version = \"1.97\""));
    assert!(file(&fs, "clippy.toml").contains("msrv = \"1.97\""));
    assert!(file(&fs, ".tool-versions").ends_with(
        "just 1.60.0\naqua:nextest-rs/nextest/cargo-nextest 0.9.144\nactionlint 1.7.12\n"
    ));
    Ok(())
}

#[test]
fn release_plz_owns_only_its_workspace_settings() -> Result<()> {
    let fs = render_preset(
        RustMonorepo::new().release_plz(ReleasePlz::new().set("semver_check", false)),
    )?;

    assert_eq!(
        file(&fs, "release-plz.toml"),
        indoc! {r#"
            [workspace]
            release_always = false
            git_release_enable = false
            git_tag_name = "{{ package }}-v{{ version }}"
            pr_branch_prefix = "release-plz/"
            pr_name = "chore: release"
            dependencies_update = true
            semver_check = false
        "#}
    );
    let workflow = file(&fs, ".github/workflows/release-plz.yml");
    assert!(workflow.contains("app-id: ${{ secrets.RELEASE_AUTOMATION_APP_ID }}"));
    assert!(workflow.contains("needs: [release-plz-release]"));
    assert!(!workflow.contains("CARGO_REGISTRY_TOKEN"));
    Ok(())
}

#[test]
fn release_plz_can_publish_with_the_default_token() -> Result<()> {
    let release_plz = ReleasePlz::new().publish_to_crates_io().github_token();

    let fs = render_preset(RustMonorepo::new().release_plz(release_plz))?;

    let workflow = file(&fs, ".github/workflows/release-plz.yml");
    assert!(!workflow.contains("app-token"));
    assert!(workflow.contains("GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}"));
    assert_eq!(
        workflow.matches("CARGO_REGISTRY_TOKEN:").count(),
        1,
        "only the release job publishes"
    );
    Ok(())
}

#[test]
fn dist_adds_config_profile_tool_recipes_and_a_ci_check() -> Result<()> {
    let fs = render_preset(RustMonorepo::new().dist(Dist::new().homebrew_tap("me/homebrew-tap")))?;

    assert!(file(&fs, "dist-workspace.toml").contains(indoc! {r#"
        installers = ["shell", "homebrew"]
        tap = "me/homebrew-tap"
        publish-jobs = ["homebrew"]
    "#}));
    assert!(
        file(&fs, "Cargo.toml")
            .ends_with("[profile.dist]\ninherits = \"release\"\nlto = \"thin\"\n")
    );
    assert!(file(&fs, ".tool-versions").ends_with("cargo-dist 0.31.0\n"));
    assert!(file(&fs, "justfile").contains("\ndist-check:\n    dist generate --check\n"));
    assert!(file(&fs, ".github/workflows/ci.yml").contains("run: just dist-check\n"));
    Ok(())
}

#[test]
fn a_customized_release_workflow_skips_the_dist_check() -> Result<()> {
    let fs = render_preset(RustMonorepo::new().dist(Dist::new().allow_dirty_ci()))?;

    assert!(file(&fs, "dist-workspace.toml").contains("allow-dirty = [\"ci\"]"));
    assert!(!file(&fs, ".github/workflows/ci.yml").contains("dist-check"));
    Ok(())
}

#[test]
fn turning_off_release_automation_cleans_it_up() -> Result<()> {
    let mut fs = render(&project(full_preset()))?;

    project(RustMonorepo::new()).synth_with(&mut fs, Mode::Write)?;

    assert_eq!(fs.get("dist-workspace.toml"), None);
    assert_eq!(fs.get(".github/workflows/release-plz.yml"), None);
    assert_eq!(fs.get("release-plz.toml"), None);
    assert!(!file(&fs, "Cargo.toml").contains("[profile.dist]"));
    assert!(!file(&fs, ".tool-versions").contains("cargo-dist"));
    Ok(())
}

fn full_preset() -> RustMonorepo {
    RustMonorepo::new()
        .repository("https://github.com/me/app")
        .release_plz(ReleasePlz::new())
        .dist(Dist::new())
}

fn project(preset: RustMonorepo) -> Project {
    let mut project = Project::new();
    project.apply(preset);
    project
}

fn file<'a>(fs: &'a MemoryFs, path: &str) -> &'a str {
    fs.get(path)
        .unwrap_or_else(|| panic!("{path} was not generated"))
}
