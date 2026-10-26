//! `cargo framer`: builds and runs a repository's `.framer/` project, and
//! creates or removes one.

use clap::{Args, Parser, Subcommand};
use framer::DiskFs;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

#[derive(Parser)]
#[command(name = "cargo", bin_name = "cargo")]
enum Cargo {
    Framer(FramerArgs),
}

/// Generate and maintain repository configuration from `.framer/src/main.rs`.
#[derive(Args)]
#[command(version)]
struct FramerArgs {
    #[command(subcommand)]
    command: Option<FramerCommand>,

    /// Write nothing; fail if any generated file is out of date.
    #[arg(long)]
    check: bool,
}

#[derive(Subcommand)]
enum FramerCommand {
    /// Create a `.framer/` project in the current directory and run it.
    Init(InitArgs),
    /// Stop using framer: keep generated files as ordinary files you own.
    Eject,
}

#[derive(Args)]
struct InitArgs {
    /// Depend on a local checkout of the framer crate instead of crates.io.
    #[arg(long, conflicts_with = "framer_git")]
    framer_path: Option<PathBuf>,

    /// Depend on the framer crate from a git repository instead of crates.io.
    #[arg(long)]
    framer_git: Option<String>,
}

const PROJECT_DIR: &str = ".framer";

const MAIN_RS: &str = r#"use framer::Project;
use std::process::ExitCode;

fn main() -> ExitCode {
    let project = Project::new();

    // Make `project` mutable and add components or presets, for example:
    //
    // use framer::github::{Ci, Job};
    // use framer::rust::{CargoWorkspace, Edition, Lints, Toolchain};
    //
    // project.add(Toolchain::stable().components(["clippy", "rustfmt"]));
    // project.add(CargoWorkspace::new().edition(Edition::E2024).lints(Lints::clippy_pedantic()));
    // project.add(Ci::new().job(Job::fmt()).job(Job::clippy()).job(Job::test()));

    project.synth()
}
"#;

fn main() -> ExitCode {
    // Cargo runs `cargo-framer framer <args>`; also accept `cargo-framer <args>`.
    let mut args: Vec<OsString> = env::args_os().collect();
    if args.get(1).is_none_or(|arg| arg != "framer") {
        args.insert(1, "framer".into());
    }
    let Cargo::Framer(args) = Cargo::parse_from(args);

    let result = match args.command {
        None => synth(args.check),
        Some(FramerCommand::Init(init_args)) => init(&init_args),
        Some(FramerCommand::Eject) => eject(),
    };
    result.unwrap_or_else(|error| {
        eprintln!("error: {error}");
        ExitCode::FAILURE
    })
}

/// The nearest directory, starting from `start`, that has a `.framer/` project.
fn find_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|dir| dir.join(PROJECT_DIR).join("Cargo.toml").is_file())
        .map(Path::to_path_buf)
}

fn current_root() -> Result<PathBuf, String> {
    let cwd = env::current_dir().map_err(|error| error.to_string())?;
    find_root(&cwd).ok_or_else(|| {
        format!(
            "no {PROJECT_DIR}/ project found in {} or its parents; run `cargo framer init` first",
            cwd.display()
        )
    })
}

fn synth(check: bool) -> Result<ExitCode, String> {
    run_project(&current_root()?, check)
}

/// Builds and runs the generator binary, which does the actual work.
fn run_project(root: &Path, check: bool) -> Result<ExitCode, String> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = Command::new(cargo);
    command
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(root.join(PROJECT_DIR).join("Cargo.toml"))
        .arg("--")
        .current_dir(root)
        .env("FRAMER_ROOT", root);
    if check {
        command.arg("--check");
    }
    let status = command
        .status()
        .map_err(|error| format!("failed to run cargo: {error}"))?;
    Ok(match status.code() {
        Some(0) => ExitCode::SUCCESS,
        Some(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        None => ExitCode::FAILURE,
    })
}

/// The `framer = ...` line for `.framer/Cargo.toml`.
fn framer_dependency(args: &InitArgs) -> String {
    if let Some(path) = &args.framer_path {
        // Relative paths are given from the repository root, but Cargo reads
        // them from `.framer/`.
        let path = if path.is_relative() {
            Path::new("..").join(path)
        } else {
            path.clone()
        };
        let path = path.to_string_lossy().replace('\\', "/");
        return format!("framer = {{ path = \"{path}\" }}");
    }
    if let Some(url) = &args.framer_git {
        return format!("framer = {{ git = \"{url}\" }}");
    }
    format!("framer = \"{}\"", env!("CARGO_PKG_VERSION"))
}

fn cargo_toml(dependency: &str) -> String {
    format!(
        r#"[package]
name = "framer-project"
version = "0.0.0"
edition = "2024"
publish = false

[dependencies]
{dependency}

# Keeps this crate out of the repository's own workspace.
[workspace]
"#
    )
}

fn write_project(root: &Path, args: &InitArgs) -> io::Result<()> {
    let project = root.join(PROJECT_DIR);
    fs::create_dir_all(project.join("src"))?;
    fs::write(
        project.join("Cargo.toml"),
        cargo_toml(&framer_dependency(args)),
    )?;
    fs::write(project.join("src/main.rs"), MAIN_RS)
}

fn init(args: &InitArgs) -> Result<ExitCode, String> {
    let root = env::current_dir().map_err(|error| error.to_string())?;
    let project = root.join(PROJECT_DIR);
    if project.exists() {
        return Err(format!("{} already exists", project.display()));
    }
    write_project(&root, args)
        .map_err(|error| format!("failed to create {PROJECT_DIR}/: {error}"))?;
    println!("Created {PROJECT_DIR}/. Describe your repository in {PROJECT_DIR}/src/main.rs.");
    run_project(&root, false)
}

fn eject() -> Result<ExitCode, String> {
    let root = current_root()?;
    let report = framer::eject(&mut DiskFs::new(&root)).map_err(|error| error.to_string())?;
    print!("{report}");
    println!("Generated files are now ordinary files. You can delete {PROJECT_DIR}/.");
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn init_args() -> InitArgs {
        InitArgs {
            framer_path: None,
            framer_git: None,
        }
    }

    #[test]
    fn finds_the_project_from_a_subdirectory() {
        let dir = TempDir::new().unwrap();
        write_project(dir.path(), &init_args()).unwrap();
        let nested = dir.path().join("crates/app/src");
        fs::create_dir_all(&nested).unwrap();

        assert_eq!(find_root(&nested).as_deref(), Some(dir.path()));
        assert_eq!(find_root(dir.path().parent().unwrap()), None);
    }

    #[test]
    fn depends_on_framer_from_crates_io_by_default() {
        assert_eq!(
            framer_dependency(&init_args()),
            format!("framer = \"{}\"", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn local_paths_are_relative_to_the_project_directory() {
        let args = InitArgs {
            framer_path: Some("../framer/crates/framer".into()),
            ..init_args()
        };

        assert_eq!(
            framer_dependency(&args),
            "framer = { path = \"../../framer/crates/framer\" }"
        );
    }

    #[test]
    fn the_project_is_its_own_workspace() {
        let dir = TempDir::new().unwrap();
        write_project(dir.path(), &init_args()).unwrap();

        let cargo_toml = fs::read_to_string(dir.path().join(".framer/Cargo.toml")).unwrap();

        assert!(cargo_toml.ends_with("[workspace]\n"));
        assert_eq!(
            fs::read_to_string(dir.path().join(".framer/src/main.rs")).unwrap(),
            MAIN_RS
        );
    }
}
