use framer::github::{Job, Step};
use framer::just::Justfile;
use framer::rust::{Level, Lints, Profile};
use framer::testing::{render, render_into, render_preset};
use framer::{MemoryFs, Mode, Project, Result};
use framer_presets::{
    Ci, CompilingJob, Dist, Release, ReleasePlz, Rust, RustMonorepo, Rustfmt, Tools, Workspace,
};
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
    let preset = RustMonorepo::new().ci(Ci::new().setup(system_libraries()));

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
    let fs =
        render_preset(RustMonorepo::new().ci(Ci::new().matrix(["ubuntu-latest", "macos-latest"])))?;

    let ci = file(&fs, ".github/workflows/ci.yml");
    assert!(
        ci.contains("        os: [ubuntu-latest, macos-latest]\n    runs-on: ${{ matrix.os }}\n")
    );
    Ok(())
}

#[test]
fn msrv_toolchain_and_tools_are_configurable() -> Result<()> {
    let preset = RustMonorepo::new()
        .rust(
            Rust::new("1.97")
                .toolchain("1.99.1")
                .targets(["wasm32-unknown-unknown"]),
        )
        .tools(Tools::new().just("1.60.0").tool("actionlint", "1.7.12"));

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
        RustMonorepo::new()
            .release(Release::new().release_plz(ReleasePlz::new().set("semver_check", false))),
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

    let fs = render_preset(RustMonorepo::new().release(Release::new().release_plz(release_plz)))?;

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
fn release_plz_publishes_with_trusted_publishing() -> Result<()> {
    let release_plz = ReleasePlz::new().publish_to_crates_io();

    let fs = render_preset(RustMonorepo::new().release(Release::new().release_plz(release_plz)))?;

    let workflow = file(&fs, ".github/workflows/release-plz.yml");
    let release = job_section(workflow, "release-plz-release");
    let release = &release[..release.find("  release-plz-pr:\n").unwrap_or(release.len())];
    assert!(release.contains("      id-token: write\n"));
    assert!(release.contains("uses: rust-lang/crates-io-auth-action@v1\n"));
    assert!(release.contains("CARGO_REGISTRY_TOKEN: ${{ steps.crates-io-auth.outputs.token }}\n"));
    assert!(!workflow.contains("secrets.CARGO_REGISTRY_TOKEN"));
    assert!(!job_section(workflow, "release-plz-pr").contains("id-token"));
    Ok(())
}

#[test]
fn release_plz_setup_steps_run_before_both_jobs_build() -> Result<()> {
    let release_plz = ReleasePlz::new().setup(system_libraries());

    let fs = render_preset(RustMonorepo::new().release(Release::new().release_plz(release_plz)))?;

    let workflow = file(&fs, ".github/workflows/release-plz.yml");
    let pr_start = workflow
        .find("  release-plz-pr:\n")
        .unwrap_or(workflow.len());
    for job in [&workflow[..pr_start], &workflow[pr_start..]] {
        let setup = job.find("Install system libraries");
        let release_plz = job.find("uses: release-plz/action@");
        assert!(setup.is_some() && setup < release_plz, "{job}");
    }
    Ok(())
}

#[test]
fn dist_adds_config_profile_tool_recipes_and_a_ci_check() -> Result<()> {
    let fs = render_preset(
        RustMonorepo::new()
            .release(Release::new().dist(Dist::new().homebrew_tap("me/homebrew-tap"))),
    )?;

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
    let fs = render_preset(
        RustMonorepo::new().release(Release::new().dist(Dist::new().allow_dirty_ci())),
    )?;

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

#[test]
fn workspace_members_lints_and_profiles_are_configurable() -> Result<()> {
    let preset = RustMonorepo::new()
        .workspace(
            Workspace::new()
                .members(["crates/ui/*", "crates/app/*"])
                .exclude(["vendor/gpui"])
                .profile(Profile::new("dev").set("debug", 1)),
        )
        .lints(Lints::clippy_pedantic().rust("unsafe_code", Level::Forbid));

    let fs = render_preset(preset)?;

    let cargo = file(&fs, "Cargo.toml");
    assert!(
        cargo.contains(
            "members = [\"crates/ui/*\", \"crates/app/*\"]\nexclude = [\"vendor/gpui\"]\n"
        )
    );
    assert!(cargo.contains("[workspace.lints.rust]\nunsafe_code = \"forbid\"\n\n"));
    assert!(!cargo.contains("unreachable_pub"));
    assert!(cargo.ends_with("[profile.dev]\ndebug = 1\n"));
    Ok(())
}

#[test]
fn rustfmt_options_extend_and_override_the_preset() -> Result<()> {
    let preset = RustMonorepo::new().rustfmt(
        Rustfmt::new()
            .set("max_width", 120)
            .set("use_try_shorthand", false),
    );

    let fs = render_preset(preset)?;

    assert!(file(&fs, "rustfmt.toml").ends_with(indoc! {r#"
        edition = "2024"
        use_field_init_shorthand = true
        use_try_shorthand = false
        max_width = 120
    "#}));
    Ok(())
}

#[test]
fn nextest_requires_the_version_mise_installs() -> Result<()> {
    let fs = render_preset(RustMonorepo::new().tools(Tools::new().nextest("0.9.150")))?;

    assert!(
        file(&fs, ".config/nextest.toml")
            .contains("nextest-version = { required = \"0.9.150\" }\n")
    );
    Ok(())
}

#[test]
fn extra_manifests_are_linted_and_formatted() -> Result<()> {
    let fs = render_preset(
        RustMonorepo::new()
            .workspace(Workspace::new().extra_manifests(["tests/fixture/Cargo.toml"])),
    )?;

    let justfile = file(&fs, "justfile");
    assert!(justfile.contains(
        "    cargo clippy --manifest-path tests/fixture/Cargo.toml --target-dir target --all-targets --all-features -- -D warnings\n"
    ));
    assert!(justfile.contains("    cargo fmt --manifest-path tests/fixture/Cargo.toml\n"));
    assert!(
        justfile.contains("    cargo fmt --manifest-path tests/fixture/Cargo.toml -- --check\n")
    );
    Ok(())
}

#[test]
fn recipes_can_be_changed_after_the_preset_is_applied() -> Result<()> {
    let mut project = project(RustMonorepo::new());
    let justfile = project
        .get_mut::<Justfile>()
        .expect("the preset adds a justfile");
    justfile.remove_recipe("test");
    justfile
        .recipe_mut("check")
        .expect("the preset has a check recipe")
        .lines_mut()
        .push("cargo check --target wasm32-unknown-unknown".into());

    let fs = render(&project)?;

    let justfile = file(&fs, "justfile");
    assert!(!justfile.contains("\ntest profile"));
    assert!(justfile.contains(
        "check:\n    cargo check --workspace --all-targets --all-features\n    cargo check --target wasm32-unknown-unknown\n"
    ));
    Ok(())
}

#[test]
fn compiling_jobs_set_up_like_the_preset_jobs() -> Result<()> {
    let preset = RustMonorepo::new().ci(Ci::new().setup(system_libraries()).compiling_job(
        CompilingJob::new("wasm", "WASM", "wasm-check")
            .timeout_minutes(25)
            .before(Step::run("cargo install trunk").name("Install trunk"))
            .step(Step::run("npm test").name("Run browser tests")),
    ));

    let fs = render_preset(preset)?;

    let ci = file(&fs, ".github/workflows/ci.yml");
    assert_eq!(ci.matches("Install system libraries").count(), 4);
    let wasm = job_section(ci, "wasm");
    assert!(wasm.starts_with("  wasm:\n    name: WASM\n"));
    assert!(wasm.contains("timeout-minutes: 25\n"));
    let order = [
        "Cache Rust builds",
        "Install system libraries",
        "Install trunk",
        "run: just wasm-check",
        "Run browser tests",
    ]
    .map(|step| {
        wasm.find(step)
            .unwrap_or_else(|| panic!("{step} is missing"))
    });
    assert!(order.is_sorted(), "{wasm}");
    Ok(())
}

#[test]
fn plain_jobs_skip_the_setup_steps() -> Result<()> {
    let preset = RustMonorepo::new().ci(Ci::new().setup(system_libraries()).job(
        Job::new("audit")
            .name("Audit")
            .step(Step::run("cargo audit")),
    ));

    let fs = render_preset(preset)?;

    let ci = file(&fs, ".github/workflows/ci.yml");
    let audit = job_section(ci, "audit");
    assert!(audit.contains("run: cargo audit\n"));
    assert!(!audit.contains("Install system libraries"));
    Ok(())
}

#[test]
fn ci_framer_check_and_conventions_are_optional() -> Result<()> {
    let without_check = render_preset(RustMonorepo::new().ci(Ci::new().without_framer_check()))?;
    let without_ci = render_preset(RustMonorepo::new().without_ci().without_conventions())?;

    assert!(!file(&without_check, ".github/workflows/ci.yml").contains("framer-check"));
    assert!(file(&without_check, "justfile").contains("\nframer-check:"));
    assert_eq!(without_ci.get(".github/workflows/ci.yml"), None);
    assert_eq!(without_ci.get("AGENTS.md"), None);
    Ok(())
}

fn full_preset() -> RustMonorepo {
    RustMonorepo::new()
        .workspace(Workspace::new().repository("https://github.com/me/app"))
        .release(
            Release::new()
                .release_plz(ReleasePlz::new())
                .dist(Dist::new()),
        )
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

fn system_libraries() -> Step {
    Step::run("sudo apt-get install --yes libfontconfig1-dev").name("Install system libraries")
}

fn job_section<'a>(workflow: &'a str, id: &str) -> &'a str {
    let start = workflow
        .find(&format!("  {id}:\n"))
        .unwrap_or_else(|| panic!("the {id} job was not generated"));
    &workflow[start..]
}
