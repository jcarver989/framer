use framer::rust::Toolchain;

/// The Rust versions the workspace supports and develops with.
///
/// The MSRV goes in `Cargo.toml` as `rust-version` and in `clippy.toml`, and
/// the toolchain in `rust-toolchain.toml`, with Clippy and rustfmt.
#[derive(Clone, Debug)]
pub struct Rust {
    msrv: String,
    toolchain: Option<String>,
    targets: Vec<String>,
}

impl Default for Rust {
    fn default() -> Self {
        Self::new("1.98")
    }
}

impl Rust {
    /// The minimum supported Rust version, such as `1.98`. Also pins the
    /// toolchain to its first release, unless [`Rust::toolchain`] says otherwise.
    pub fn new(msrv: &str) -> Self {
        Self {
            msrv: msrv.into(),
            toolchain: None,
            targets: Vec::new(),
        }
    }

    /// The exact toolchain to develop and test with, such as `1.99.1`.
    pub fn toolchain(mut self, version: &str) -> Self {
        self.toolchain = Some(version.into());
        self
    }

    /// Extra compilation targets to install, such as `wasm32-unknown-unknown`.
    pub fn targets<T: Into<String>>(mut self, triples: impl IntoIterator<Item = T>) -> Self {
        self.targets.extend(triples.into_iter().map(Into::into));
        self
    }

    pub(crate) fn msrv(&self) -> &str {
        &self.msrv
    }

    pub(crate) fn toolchain_file(&self) -> Toolchain {
        let channel = self
            .toolchain
            .clone()
            .unwrap_or_else(|| format!("{}.0", self.msrv));
        Toolchain::pinned(channel)
            .components(["clippy", "rustfmt"])
            .targets(self.targets.clone())
    }
}
