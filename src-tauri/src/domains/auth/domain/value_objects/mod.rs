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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_role_serializes_as_lowercase_string() {
        assert_eq!(
            serde_json::to_string(&UserRole::Superadmin).unwrap(),
            "\"superadmin\""
        );
        assert_eq!(
            serde_json::to_string(&UserRole::Receptionist).unwrap(),
            "\"receptionist\""
        );
        assert_eq!(
            serde_json::to_string(&UserRole::Accountant).unwrap(),
            "\"accountant\""
        );
    }

    #[test]
    fn user_role_deserializes_from_lowercase_string() {
        let r: UserRole = serde_json::from_str("\"accountant\"").unwrap();
        assert_eq!(r, UserRole::Accountant);
    }

    #[test]
    fn user_role_rejects_unknown_value() {
        assert!(serde_json::from_str::<UserRole>("\"shareholder\"").is_err());
    }

    #[test]
    fn user_role_as_str_round_trips_through_parse() {
        for r in [
            UserRole::Superadmin,
            UserRole::Receptionist,
            UserRole::Accountant,
        ] {
            assert_eq!(UserRole::parse(r.as_str()).unwrap(), r);
        }
        assert!(UserRole::parse("god-mode").is_none());
    }

    #[test]
    fn user_role_display_uses_as_str() {
        assert_eq!(format!("{}", UserRole::Superadmin), "superadmin");
    }

    #[test]
    fn login_mode_serializes_as_lowercase() {
        assert_eq!(
            serde_json::to_string(&LoginMode::Online).unwrap(),
            "\"online\""
        );
        assert_eq!(
            serde_json::to_string(&LoginMode::Offline).unwrap(),
            "\"offline\""
        );
    }
}
