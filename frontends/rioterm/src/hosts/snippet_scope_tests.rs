use super::*;

#[test]
fn local_snippets_are_global() {
    assert_eq!(snippet_scope(Some(LOCAL_ID)), None);
    assert_eq!(snippet_scope(None), None);
}

#[test]
fn host_snippets_keep_their_host() {
    assert_eq!(snippet_scope(Some("host-1")), Some("host-1".to_string()));
}
