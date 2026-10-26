//! `.framer/manifest.json` records what the last run generated, so the next
//! run can delete or shrink files that are no longer generated.

use crate::error::{FramerError, Result};
use crate::fs::FileSystem;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub(crate) const MANIFEST_PATH: &str = ".framer/manifest.json";

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct Manifest {
    pub(crate) files: BTreeMap<String, Entry>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Entry {
    /// framer owns the whole file.
    Managed,
    /// framer owns these dotted keys in a TOML file.
    Toml { keys: Vec<String> },
    /// framer owns these marked blocks in a text file.
    Blocks { ids: Vec<String> },
}

impl Manifest {
    pub(crate) fn load(fs: &dyn FileSystem) -> Result<Self> {
        let path = Path::new(MANIFEST_PATH);
        match fs.read(path)? {
            Some(contents) => {
                serde_json::from_str(&contents).map_err(|source| FramerError::Manifest {
                    path: path.to_owned(),
                    source,
                })
            }
            None => Ok(Self::default()),
        }
    }

    pub(crate) fn to_json(&self) -> String {
        let mut json = serde_json::to_string_pretty(self).expect("a manifest always serializes");
        json.push('\n');
        json
    }
}
