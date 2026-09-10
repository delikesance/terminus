//! SSH `gssapi-with-mic` (RFC 4462) using the OS Kerberos ticket cache.

use crate::error::{Error, Result};
use crate::models::Host;
use crate::ssh::HostKeyVerifier;
use russh::client::Handle;

/// Kerberos V5 mechanism OID (RFC 1964) in DER.
pub const KRB5_MECH_OID_DER: &[u8] = &[
    0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x12, 0x01, 0x02, 0x02,
];

pub const AUTH_METHOD_GSSAPI: &str = "gssapi";

pub fn is_gssapi_method(method: &str) -> bool {
    method.eq_ignore_ascii_case(AUTH_METHOD_GSSAPI)
}

/// RFC 4462 host-based service name: `host@<hostname>`.
pub fn host_based_service_name(hostname: &str) -> String {
    format!("host@{hostname}")
}

pub fn map_gss_error(msg: &str) -> Error {
    let lower = msg.to_ascii_lowercase();
    if lower.contains("no credentials cache")
        || lower.contains("no kerberos credentials")
        || lower.contains("ticket expired")
        || lower.contains("krb5krb_ap_err_tkt_expired")
        || lower.contains("credentials expired")
        || lower.contains("gss_s_no_cred")
    {
        return Error::GssapiNoTicket;
    }
    Error::msg(msg.to_string())
}

pub async fn authenticate(session: &mut Handle<HostKeyVerifier>, host: &Host) -> Result<bool> {
    #[cfg(windows)]
    {
        let _ = session;
        let _ = host;
        return Err(Error::GssapiUnsupported);
    }
    #[cfg(unix)]
    {
        unix::authenticate(session, host).await
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = session;
        let _ = host;
        Err(Error::GssapiUnsupported)
    }
}

#[cfg(unix)]
mod unix {
    use super::*;
    use libgssapi::context::{ClientCtx, CtxFlags, SecurityContext};
    use libgssapi::name::Name;
    use libgssapi::oid::{GSS_MECH_KRB5, GSS_NT_HOSTBASED_SERVICE};
    use russh::{GssapiAuthenticator, GssapiStep};

    pub async fn authenticate(session: &mut Handle<HostKeyVerifier>, host: &Host) -> Result<bool> {
        let mut auth = KerberosGssapi::new(host.hostname.clone());
        let result = session
            .authenticate_gssapi_with_mic(
                host.username.clone(),
                vec![KRB5_MECH_OID_DER.to_vec()],
                &mut auth,
            )
            .await
            .map_err(|e| map_gss_error(&e.to_string()))?;
        Ok(result.success())
    }

    struct KerberosGssapi {
        hostname: String,
        ctx: Option<ClientCtx>,
    }

    impl KerberosGssapi {
        fn new(hostname: String) -> Self {
            Self {
                hostname,
                ctx: None,
            }
        }

        fn offered_mech(oid: &[u8]) -> bool {
            oid == KRB5_MECH_OID_DER || oid == &KRB5_MECH_OID_DER[2..]
        }

        fn step_inner(
            &mut self,
            selected_mechanism: Option<Vec<u8>>,
            input_token: Option<Vec<u8>>,
            mic_data: &[u8],
        ) -> Result<GssapiStep> {
            if let Some(mech) = selected_mechanism.as_deref() {
                if !Self::offered_mech(mech) {
                    return Err(Error::msg("SSH server selected an unsupported GSSAPI mechanism"));
                }
            }
            if self.ctx.is_none() {
                let target = host_based_service_name(&self.hostname);
                let name = Name::new(target.as_bytes(), Some(GSS_NT_HOSTBASED_SERVICE))
                    .map_err(|e| map_gss_error(&e.to_string()))?;
                self.ctx = Some(ClientCtx::new(
                    None,
                    name,
                    CtxFlags::GSS_C_MUTUAL_FLAG | CtxFlags::GSS_C_INTEG_FLAG,
                    Some(GSS_MECH_KRB5),
                ));
            }
            let ctx = self.ctx.as_mut().expect("ctx");
            let token = ctx
                .step(input_token.as_deref(), None)
                .map_err(|e| map_gss_error(&e.to_string()))?;
            if ctx.is_complete() {
                let mic = ctx
                    .get_mic(mic_data)
                    .map_err(|e| map_gss_error(&e.to_string()))?;
                Ok(GssapiStep::Complete {
                    token: token.map(|t| t.to_vec()),
                    mic: Some(mic.to_vec()),
                })
            } else {
                let token = token.ok_or_else(|| {
                    Error::msg("GSSAPI continue needed but no token was produced")
                })?;
                Ok(GssapiStep::Continue {
                    token: token.to_vec(),
                })
            }
        }
    }

    impl GssapiAuthenticator for KerberosGssapi {
        type Error = Error;

        fn gssapi_step(
            &mut self,
            selected_mechanism: Option<Vec<u8>>,
            input_token: Option<Vec<u8>>,
            mic_data: Vec<u8>,
        ) -> impl std::future::Future<Output = std::result::Result<GssapiStep, Self::Error>> + Send
        {
            let result = self.step_inner(selected_mechanism, input_token, &mic_data);
            async move { result }
        }
    }
}
