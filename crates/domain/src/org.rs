use std::fmt;

use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OrgScope(Uuid);

impl OrgScope {
    pub const fn new(id: Uuid) -> Self {
        Self(id)
    }

    pub const fn get(self) -> Uuid {
        self.0
    }
}

impl From<Uuid> for OrgScope {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl From<OrgScope> for Uuid {
    fn from(org: OrgScope) -> Self {
        org.0
    }
}

impl fmt::Display for OrgScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
