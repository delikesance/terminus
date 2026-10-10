use super::test_support::{content, free, item};
use super::*;
use crate::components::input::TextDraft;
use crate::geom::Rect;
#[test]
fn list_and_dialog_controls_show_a_hand_fields_an_i_beam() {
    use crate::chrome::ChromeCursor;
    let mut s = TunnelsState::new("jerem prod");
    s.items = vec![item("a", TunnelKind::Local, TunnelStatus::Running)];
    let m = s.metrics.get();
    let mid = |r: Rect| (r.x + r.width / 2.0, r.y + r.height / 2.0);
    let l = list_layout(content(), 1, &m);
    let (x, y) = mid(l.new_button);
    assert_eq!(s.cursor_at(content(), x, y), ChromeCursor::Pointer);
    let (x, y) = mid(l.cards[0]);
    assert_eq!(s.cursor_at(content(), x, y), ChromeCursor::Pointer);
    assert_eq!(
        s.cursor_at(content(), content().x + 2.0, content().bottom() - 2.0),
        ChromeCursor::Default
    );

    s.open_new();
    let form = s.form.clone().unwrap();
    let d = dialog_layout(content(), &form, &m);
    let (x, y) = mid(d.confirm);
    assert_eq!(s.cursor_at(content(), x, y), ChromeCursor::Pointer);
    let (x, y) = mid(d.cancel);
    assert_eq!(s.cursor_at(content(), x, y), ChromeCursor::Pointer);
    let (x, y) = mid(d.local.box_rect);
    assert_eq!(s.cursor_at(content(), x, y), ChromeCursor::Text);
    let (x, y) = mid(d.title);
    assert_eq!(s.cursor_at(content(), x, y), ChromeCursor::Default);
}

#[test]
fn state_press_flow_opens_validates_and_saves() {
    let mut s = TunnelsState::new("jerem prod");
    let l = list_layout(content(), 0, &s.metrics.get());
    let n = l.new_button;
    assert_eq!(
        s.press(content(), n.x + 3.0, n.y + 3.0, &free),
        TunnelAction::Redraw
    );
    assert!(s.form.is_some());
    {
        let f = s.form.as_mut().unwrap();
        f.local_port = TextDraft::new("5432");
        f.dest_host = TextDraft::new("localhost");
        f.dest_port = TextDraft::new("5432");
    }
    let d = dialog_layout(content(), s.form.as_ref().unwrap(), &s.metrics.get());
    let c = d.confirm;
    match s.press(content(), c.x + 3.0, c.y + 3.0, &free) {
        TunnelAction::Save(draft) => assert_eq!(draft.bind_port, 5432),
        other => panic!("{other:?}"),
    }
    assert!(s.form.is_none(), "the dialog closes on a valid save");
}

#[test]
fn an_invalid_save_keeps_the_dialog_open() {
    let mut s = TunnelsState::new("h");
    s.form = Some(TunnelForm::new("h"));
    let d = dialog_layout(content(), s.form.as_ref().unwrap(), &s.metrics.get());
    let c = d.confirm;
    assert_eq!(
        s.press(content(), c.x + 3.0, c.y + 3.0, &free),
        TunnelAction::Redraw
    );
    assert!(s.form.is_some());
    assert!(s
        .form
        .as_ref()
        .unwrap()
        .error_for(FormField::LocalPort)
        .is_some());
}

#[test]
fn cancel_and_escape_close_the_dialog() {
    let mut s = TunnelsState::new("h");
    s.form = Some(TunnelForm::new("h"));
    assert_eq!(s.key(FormKey::Escape, &free), TunnelAction::Redraw);
    assert!(s.form.is_none());
    s.form = Some(TunnelForm::new("h"));
    let d = dialog_layout(content(), s.form.as_ref().unwrap(), &s.metrics.get());
    s.press(content(), d.cancel.x + 3.0, d.cancel.y + 3.0, &free);
    assert!(s.form.is_none());
}

#[test]
fn toggle_delete_and_edit_become_actions() {
    let mut s = TunnelsState::new("h");
    s.items = vec![item("a", TunnelKind::Local, TunnelStatus::Stopped)];
    let m = s.metrics.get();
    let l = list_layout(content(), 1, &m);
    let cl = card_geometry(l.cards[0], &m, TunnelStatus::Stopped);
    let a = cl.actions[0].unwrap();
    assert_eq!(
        s.press(content(), a.x + 3.0, a.y + 3.0, &free),
        TunnelAction::Toggle("a".into())
    );
    let t = cl.actions[1].unwrap();
    assert_eq!(
        s.press(content(), t.x + 3.0, t.y + 3.0, &free),
        TunnelAction::Delete("a".into())
    );
    let b = l.cards[0];
    assert_eq!(
        s.press(content(), b.x + 30.0, b.y + 5.0, &free),
        TunnelAction::Redraw
    );
    assert_eq!(s.form.as_ref().and_then(|f| f.id.clone()), Some("a".into()));
}

#[test]
fn hover_reports_whether_it_changed() {
    let mut s = TunnelsState::new("h");
    s.items = vec![item("a", TunnelKind::Local, TunnelStatus::Stopped)];
    let l = list_layout(content(), 1, &s.metrics.get());
    let b = l.cards[0];
    assert!(s.hover(content(), b.x + 30.0, b.y + 5.0));
    assert!(!s.hover(content(), b.x + 31.0, b.y + 5.0));
    assert!(s.hover(content(), 1.0, 1.0));
}
