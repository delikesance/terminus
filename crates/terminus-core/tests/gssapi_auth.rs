//! GSSAPI / Kerberos SSH auth contracts (issue #113).

use terminus_core::error::Error;
use terminus_core::gssapi::{
    host_based_service_name, is_gssapi_method, map_gss_error, KRB5_MECH_OID_DER,
};

#[test]
fn host_based_service_name_follows_rfc4462() {
    assert_eq!(
        host_based_service_name("box.example.com"),
        "host@box.example.com"
    );
}

#[test]
fn krb5_mechanism_oid_is_rfc1964_der() {
    assert_eq!(
        KRB5_MECH_OID_DER,
        &[0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x12, 0x01, 0x02, 0x02]
    );
}

#[test]
fn gssapi_method_is_not_password_or_key() {
    assert!(is_gssapi_method("gssapi"));
    assert!(!is_gssapi_method("password"));
    assert!(!is_gssapi_method("key"));
    assert!(!is_gssapi_method("agent"));
    assert!(!is_gssapi_method("kerberos"));
}

#[test]
fn no_ticket_error_copy_is_exact() {
    assert_eq!(
        Error::GssapiNoTicket.to_string(),
        "No Kerberos ticket found. Run kinit, then try again."
    );
}

#[test]
fn windows_unsupported_error_copy_is_exact() {
    assert_eq!(
        Error::GssapiUnsupported.to_string(),
        "Kerberos (GSSAPI) is not supported on Windows."
    );
}

#[test]
fn missing_or_expired_cache_maps_to_no_ticket() {
    for needle in [
        "No credentials cache found",
        "No Kerberos credentials available",
        "Ticket expired",
        "KRB5KRB_AP_ERR_TKT_EXPIRED",
    ] {
        let err = map_gss_error(needle);
        assert!(
            matches!(err, Error::GssapiNoTicket),
            "{needle} should be GssapiNoTicket, got {err}"
        );
    }
}

#[test]
fn other_gss_errors_stay_generic() {
    let err = map_gss_error("Server not found in Kerberos database");
    assert!(!matches!(err, Error::GssapiNoTicket));
    assert!(!matches!(err, Error::GssapiUnsupported));
}
