//! Small, nonsecret recovery observations. No token, client or idle task is kept.
use super::{before_dispatch, AuthError, ExecutionFailure, FailureClass, REGISTRATION_RECOVERY};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Recovery {
    Reconnect,
    Registration,
    Configuration,
    CredentialStore,
    Temporary,
}

impl Recovery {
    pub(super) fn from_auth(error: &AuthError) -> Self {
        match error {
            AuthError::AuthorizationRequired
            | AuthError::TokenRefreshRejected(_)
            | AuthError::TokenExpired => Self::Reconnect,
            AuthError::CredentialStoreError(_) => Self::CredentialStore,
            AuthError::PkceUnsupported
            | AuthError::InvalidScope(_)
            | AuthError::InsufficientScope { .. }
            | AuthError::NoAuthorizationSupport => Self::Configuration,
            // SDK strings may contain provider data or secrets. Never classify
            // by matching their contents, or expose them to the model/UI.
            _ => Self::Temporary,
        }
    }

    pub(super) fn state(self) -> &'static str {
        match self {
            Self::Reconnect => "reconnect_required",
            Self::Registration => "needs_client_registration",
            Self::Configuration => "configuration_required",
            Self::CredentialStore => "credential_store_unavailable",
            Self::Temporary => "temporarily_unavailable",
        }
    }

    pub(super) fn message(self) -> &'static str {
        match self {
            Self::Reconnect => "The MCP account needs sign-in again. Reconnect in Grain Settings.",
            Self::Registration => REGISTRATION_RECOVERY,
            Self::Configuration => "The MCP authorization setup is unsupported or changed. Check the provider and client configuration in Grain Settings.",
            Self::CredentialStore => "The OS credential vault is unavailable. Keep this connection and try again after the vault is available.",
            Self::Temporary => "MCP authentication is temporarily unavailable. Keep this connection and try again later.",
        }
    }

    pub(super) fn failure(self) -> ExecutionFailure {
        before_dispatch(
            if self == Self::Temporary {
                FailureClass::Network
            } else {
                FailureClass::Auth
            },
            self.message(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_definitive_rejection_requires_reconnect() {
        assert_eq!(
            Recovery::from_auth(&AuthError::AuthorizationRequired),
            Recovery::Reconnect
        );
        assert_eq!(
            Recovery::from_auth(&AuthError::TokenRefreshRejected("private".into())),
            Recovery::Reconnect
        );
        for error in [
            AuthError::TokenRefreshFailed("invalid_grant private".into()),
            AuthError::InternalError("private".into()),
            AuthError::MetadataError("private".into()),
        ] {
            let recovery = Recovery::from_auth(&error);
            assert_eq!(recovery, Recovery::Temporary);
            assert!(!recovery.message().contains("private"));
        }
        assert_eq!(
            Recovery::from_auth(&AuthError::CredentialStoreError("private".into())),
            Recovery::CredentialStore
        );
        assert_eq!(
            Recovery::from_auth(&AuthError::PkceUnsupported),
            Recovery::Configuration
        );
    }
}
