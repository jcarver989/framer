use crate::{Dist, ReleasePlz};
use framer::Project;

/// Release automation, off by default: release PRs and tags with
/// [`ReleasePlz`], and binary releases with [`Dist`].
#[derive(Clone, Debug, Default)]
pub struct Release {
    release_plz: Option<ReleasePlz>,
    dist: Option<Dist>,
}

impl Release {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn release_plz(mut self, release_plz: ReleasePlz) -> Self {
        self.release_plz = Some(release_plz);
        self
    }

    pub fn dist(mut self, dist: Dist) -> Self {
        self.dist = Some(dist);
        self
    }

    pub(crate) fn dist_config(&self) -> Option<&Dist> {
        self.dist.as_ref()
    }

    pub(crate) fn apply(&self, project: &mut Project) {
        if let Some(release_plz) = &self.release_plz {
            release_plz.apply(project);
        }
        if let Some(dist) = &self.dist {
            project.add(dist.config());
        }
    }
}
