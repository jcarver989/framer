//! The steps every generated workflow shares. Tools come from `.tool-versions`
//! through mise, and Rust from `rust-toolchain.toml`.

use framer::github::{Job, Step};

const CHECKOUT: &str = "actions/checkout@v7";
const MISE: &str = "jdx/mise-action@v5.2.0";
const RUST_CACHE: &str = "Swatinem/rust-cache@v2";

pub(crate) fn checkout() -> Step {
    Step::uses(CHECKOUT).name("Checkout repository")
}

pub(crate) fn install_tools() -> Step {
    Step::uses(MISE).name("Install tools")
}

pub(crate) fn rust_cache() -> Step {
    Step::uses(RUST_CACHE).name("Cache Rust builds")
}

/// Pull requests reuse the cache from `main` but don't write their own.
fn rust_cache_saved_on_main() -> Step {
    rust_cache().with("save-if", "${{ github.ref == 'refs/heads/main' }}")
}

/// A job that runs `just <recipe>`.
pub(crate) fn just_job(id: &str, name: &str, recipe: &str, timeout_minutes: u32) -> Job {
    Job::new(id)
        .name(name)
        .timeout_minutes(timeout_minutes)
        .step(checkout())
        .step(install_tools())
        .step(Step::run(format!("just {recipe}")))
}

/// A `just_job` that compiles the workspace, so it caches builds and runs the
/// repository's extra setup steps first.
pub(crate) fn compiling_job(
    id: &str,
    name: &str,
    recipe: &str,
    timeout_minutes: u32,
    setup: &[Step],
) -> Job {
    let mut job = Job::new(id)
        .name(name)
        .timeout_minutes(timeout_minutes)
        .step(checkout())
        .step(install_tools())
        .step(rust_cache_saved_on_main());
    for step in setup {
        job = job.step(step.clone());
    }
    job.step(Step::run(format!("just {recipe}")))
}
