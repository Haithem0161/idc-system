//! Value objects for the auth bounded context.

use serde::{Deserialize, Serialize};

/// Role granted to a user. Maps onto the Prisma `UserRole` enum and the
/// SQLite `users.role` CHECK.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    Superadmin,
    Receptionist,
    Accountant,
}

impl UserRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Superadmin => "superadmin",
            Self::Receptionist => "receptionist",
            Self::Accountant => "accountant",
        }
    }

    pub fn parse(input: &str) -> Option<Self> {
        match input {
            "superadmin" => Some(Self::Superadmin),
            "receptionist" => Some(Self::Receptionist),
            "accountant" => Some(Self::Accountant),
            _ => None,
        }
    }
}

impl std::fmt::Display for UserRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The mode reported back by `auth::login`. Online means the server validated
/// the credentials and issued tokens; offline means we verified locally
/// against the cached row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoginMode {
    Online,
    Offline,
}
