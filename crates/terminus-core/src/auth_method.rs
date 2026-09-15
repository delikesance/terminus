//! Host SSH authentication methods — fail-closed parse (unknown ≠ password).
//!
//! Mirrors the production `authMethod.ts` contract so UI and store agree.

/// Supported SSH auth methods for a stored host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostAuthMethod {
    /// Public-key auth using a saved [`crate::models::Identity`].
    Key,
    /// Password auth; secret lives in the vault, not on the host row.
    Password,
    /// Kerberos GSSAPI (`gssapi-with-mic`) via the OS ticket cache.
    Gssapi,
}

impl HostAuthMethod {
    /// Canonical store / wire value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::Password => "password",
            Self::Gssapi => "gssapi",
        }
    }

    /// Human label for the add-host selector.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Key => "SSH Cryptographic Key",
            Self::Password => "Password Authentication",
            Self::Gssapi => "Kerberos (GSSAPI)",
        }
    }
}

impl std::fmt::Display for HostAuthMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Successful parse of a host auth method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseHostAuthOk {
    pub method: HostAuthMethod,
}

/// Fail-closed: unknown raw values are never treated as password.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseHostAuthErr {
    pub raw: String,
}

/// Result of [`parse_host_auth_method`].
pub type ParseHostAuthResult = Result<ParseHostAuthOk, ParseHostAuthErr>;

/// Parse a stored / UI auth method string. Unknown ≠ password.
pub fn parse_host_auth_method(raw: &str) -> ParseHostAuthResult {
    match raw.trim().to_ascii_lowercase().as_str() {
        "key" => Ok(ParseHostAuthOk {
            method: HostAuthMethod::Key,
        }),
        "password" => Ok(ParseHostAuthOk {
            method: HostAuthMethod::Password,
        }),
        "gssapi" => Ok(ParseHostAuthOk {
            method: HostAuthMethod::Gssapi,
        }),
        _ => Err(ParseHostAuthErr {
            raw: raw.to_string(),
        }),
    }
}

/// Which conditional fields the add-host form should show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostAuthUi {
    pub method: HostAuthMethod,
    pub show_key: bool,
    pub show_password: bool,
}

/// UI flags for a (possibly unknown) method string. Unknown falls back to key.
pub fn host_auth_ui(method: &str) -> HostAuthUi {
    match parse_host_auth_method(method) {
        Ok(ParseHostAuthOk {
            method: HostAuthMethod::Gssapi,
        }) => HostAuthUi {
            method: HostAuthMethod::Gssapi,
            show_key: false,
            show_password: false,
        },
        Ok(ParseHostAuthOk {
            method: HostAuthMethod::Password,
        }) => HostAuthUi {
            method: HostAuthMethod::Password,
            show_key: false,
            show_password: true,
        },
        Ok(ParseHostAuthOk {
            method: HostAuthMethod::Key,
        })
        | Err(_) => HostAuthUi {
            method: HostAuthMethod::Key,
            show_key: true,
            show_password: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_the_three_canonical_methods() {
        assert_eq!(
            parse_host_auth_method("key").unwrap().method,
            HostAuthMethod::Key
        );
        assert_eq!(
            parse_host_auth_method("PASSWORD").unwrap().method,
            HostAuthMethod::Password
        );
        assert_eq!(
            parse_host_auth_method(" gssapi ").unwrap().method,
            HostAuthMethod::Gssapi
        );
    }

    #[test]
    fn parse_rejects_unknown_without_defaulting_to_password() {
        let err = parse_host_auth_method("agent").unwrap_err();
        assert_eq!(err.raw, "agent");
        assert!(parse_host_auth_method("").is_err());
        assert!(parse_host_auth_method("kerberos").is_err());
    }

    #[test]
    fn host_auth_ui_toggles_conditional_fields() {
        assert_eq!(
            host_auth_ui("key"),
            HostAuthUi {
                method: HostAuthMethod::Key,
                show_key: true,
                show_password: false,
            }
        );
        assert_eq!(
            host_auth_ui("password"),
            HostAuthUi {
                method: HostAuthMethod::Password,
                show_key: false,
                show_password: true,
            }
        );
        assert_eq!(
            host_auth_ui("gssapi"),
            HostAuthUi {
                method: HostAuthMethod::Gssapi,
                show_key: false,
                show_password: false,
            }
        );
        // Unknown → key UI (fail closed for secrets).
        assert!(host_auth_ui("auto").show_key);
        assert!(!host_auth_ui("auto").show_password);
    }

    #[test]
    fn as_str_round_trips_through_parse() {
        for method in [
            HostAuthMethod::Key,
            HostAuthMethod::Password,
            HostAuthMethod::Gssapi,
        ] {
            assert_eq!(
                parse_host_auth_method(method.as_str()).unwrap().method,
                method
            );
        }
    }
}
