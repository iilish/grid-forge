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
    fn from(scope: OrgScope) -> Self {
        scope.0
    }
}

impl std::fmt::Display for OrgScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
