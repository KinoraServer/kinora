//! User roles. Every API route declares the minimum role it needs.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Can watch.
    Viewer = 0,
    /// Can also fix metadata.
    Editor = 1,
    /// Can manage libraries, users and settings.
    Admin = 2,
    /// The person who set up the server. Cannot be deleted or demoted by admins.
    Owner = 3,
}

impl Role {
    pub fn at_least(self, required: Role) -> bool {
        self >= required
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Viewer => "viewer",
            Role::Editor => "editor",
            Role::Admin => "admin",
            Role::Owner => "owner",
        }
    }

    pub fn parse(s: &str) -> Option<Role> {
        match s {
            "viewer" => Some(Role::Viewer),
            "editor" => Some(Role::Editor),
            "admin" => Some(Role::Admin),
            "owner" => Some(Role::Owner),
            _ => None,
        }
    }

    /// Can `self` create or change a user that has role `target`?
    /// Admins manage everyone below owner; only the owner can make admins.
    pub fn can_manage(self, target: Role) -> bool {
        match self {
            Role::Owner => target != Role::Owner,
            Role::Admin => target < Role::Admin,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordering() {
        assert!(Role::Owner.at_least(Role::Admin));
        assert!(Role::Admin.at_least(Role::Admin));
        assert!(!Role::Editor.at_least(Role::Admin));
        assert!(Role::Viewer.at_least(Role::Viewer));
    }

    #[test]
    fn management() {
        assert!(Role::Owner.can_manage(Role::Admin));
        assert!(!Role::Owner.can_manage(Role::Owner));
        assert!(Role::Admin.can_manage(Role::Editor));
        assert!(!Role::Admin.can_manage(Role::Admin));
        assert!(!Role::Editor.can_manage(Role::Viewer));
    }

    #[test]
    fn round_trip() {
        for r in [Role::Viewer, Role::Editor, Role::Admin, Role::Owner] {
            assert_eq!(Role::parse(r.as_str()), Some(r));
        }
        assert_eq!(Role::parse("root"), None);
    }
}
