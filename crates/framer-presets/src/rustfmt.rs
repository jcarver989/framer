use framer::TomlFile;
use framer::toml_edit::Value;

/// `rustfmt.toml`.
#[derive(Clone, Debug)]
pub struct Rustfmt {
    file: TomlFile,
}

impl Default for Rustfmt {
    fn default() -> Self {
        Self::new()
    }
}

impl Rustfmt {
    /// Edition 2024, with field-init and `?` shorthand.
    pub fn new() -> Self {
        Self {
            file: TomlFile::new("rustfmt.toml")
                .set("edition", "2024")
                .set("use_field_init_shorthand", true)
                .set("use_try_shorthand", true),
        }
    }

    /// Sets an option, such as `max_width`, replacing the preset's value if it has one.
    pub fn set(mut self, option: &str, setting: impl Into<Value>) -> Self {
        self.file = self.file.set(option, setting);
        self
    }

    pub(crate) fn file(&self) -> TomlFile {
        self.file.clone()
    }
}
