use super::test_support::{free, item};
use super::*;
use crate::components::input::{TextDraft, TextEdit, TextMoveKind};
#[test]
fn a_valid_local_form_yields_a_draft() {
    let mut f = TunnelForm::new("jerem prod");
    f.name = TextDraft::new("Database");
    f.local_port = TextDraft::new("5432");
    f.dest_host = TextDraft::new("localhost");
    f.dest_port = TextDraft::new("5432");
    let d = f.validate(&free).expect("valid");
    assert_eq!(d.kind, TunnelKind::Local);
    assert_eq!(d.name, "Database");
    assert_eq!((d.bind_port, d.dest_port), (5432, 5432));
    assert_eq!(d.bind_host, "127.0.0.1");
    assert!(d.id.is_none());
}

#[test]
fn an_empty_name_falls_back_to_the_route() {
    let mut f = TunnelForm::new("h");
    f.local_port = TextDraft::new("8080");
    f.dest_host = TextDraft::new("localhost");
    f.dest_port = TextDraft::new("80");
    assert_eq!(
        f.validate(&free).unwrap().name,
        "localhost:8080 \u{2192} localhost:80"
    );
}

#[test]
fn bad_ports_and_hosts_flag_their_fields() {
    let mut f = TunnelForm::new("h");
    f.local_port = TextDraft::new("0");
    f.dest_host = TextDraft::new("bad host");
    f.dest_port = TextDraft::new("70000");
    assert!(f.validate(&free).is_err());
    assert!(f.error_for(FormField::LocalPort).is_some());
    assert!(f.error_for(FormField::DestHost).is_some());
    assert!(f.error_for(FormField::DestPort).is_some());
    assert!(f.error_for(FormField::Name).is_none());
}

#[test]
fn a_busy_local_port_is_refused_but_not_for_remote_forwards() {
    let busy = |p: u16| p != 5432;
    let mut f = TunnelForm::new("h");
    f.local_port = TextDraft::new("5432");
    f.dest_host = TextDraft::new("localhost");
    f.dest_port = TextDraft::new("5432");
    assert!(f.validate(&busy).is_err());
    assert!(f
        .error_for(FormField::LocalPort)
        .unwrap()
        .contains("in use"));
    f.set_kind(TunnelKind::Remote);
    assert!(
        f.validate(&busy).is_ok(),
        "the remote port cannot be probed locally"
    );
}

#[test]
fn dynamic_needs_only_a_local_port() {
    let mut f = TunnelForm::new("h");
    f.set_kind(TunnelKind::Dynamic);
    assert_eq!(f.fields(), vec![FormField::Name, FormField::LocalPort]);
    f.local_port = TextDraft::new("1080");
    let d = f.validate(&free).expect("valid without a destination");
    assert_eq!(d.kind, TunnelKind::Dynamic);
    assert_eq!(d.dest_host, "");
    assert_eq!(d.dest_port, 0);
}

#[test]
fn editing_keeps_the_id_and_fields() {
    let mut t = item("abc", TunnelKind::Remote, TunnelStatus::Stopped);
    t.dest_host = "10.0.0.5".into();
    let mut f = TunnelForm::editing(&t, "h");
    assert_eq!(f.kind, TunnelKind::Remote);
    assert_eq!(f.dest_host.value, "10.0.0.5");
    let d = f.validate(&free).unwrap();
    assert_eq!(d.id.as_deref(), Some("abc"));
}

#[test]
fn typing_is_filtered_per_field() {
    let mut f = TunnelForm::new("h");
    f.focus = FormField::LocalPort;
    for c in "12ab345678".chars() {
        f.type_char(c);
    }
    assert_eq!(f.local_port.value, "12345", "digits only, 5 max");
    f.focus = FormField::DestHost;
    f.type_char(' ');
    f.type_char('a');
    assert_eq!(f.dest_host.value, "localhosta");
    f.focus = FormField::Name;
    f.type_char('x');
    f.backspace();
    f.backspace();
    assert_eq!(f.name.value, "");
}

#[test]
fn port_fields_support_delete_caret_and_select_all() {
    let mut f = TunnelForm::new("h");
    f.focus = FormField::LocalPort;
    f.insert_text("5432");
    f.key(FormKey::Edit(TextEdit::Home {
        kind: TextMoveKind::Collapse,
    }));
    f.key(FormKey::Edit(TextEdit::Delete { by_word: false }));
    assert_eq!(f.local_port.value, "432");
    f.key(FormKey::Edit(TextEdit::SelectAll));
    f.key(FormKey::Edit(TextEdit::Backspace { by_word: false }));
    assert_eq!(f.local_port.value, "");
}

#[test]
fn tab_cycles_the_visible_fields() {
    let mut f = TunnelForm::new("h");
    assert_eq!(f.focus, FormField::Name);
    f.key(FormKey::Tab);
    assert_eq!(f.focus, FormField::LocalPort);
    f.key(FormKey::Tab);
    f.key(FormKey::Tab);
    f.key(FormKey::Tab);
    assert_eq!(f.focus, FormField::Name, "wraps");
    f.key(FormKey::BackTab);
    assert_eq!(f.focus, FormField::DestPort);
    f.set_kind(TunnelKind::Dynamic);
    assert_eq!(f.focus, FormField::Name, "a hidden field loses focus");
}

#[test]
fn enter_submits_and_escape_cancels() {
    let mut f = TunnelForm::new("h");
    assert_eq!(f.key(FormKey::Enter), FormOutcome::Submit);
    assert_eq!(f.key(FormKey::Escape), FormOutcome::Cancel);
    assert_eq!(f.key(FormKey::Char('a')), FormOutcome::None);
}
