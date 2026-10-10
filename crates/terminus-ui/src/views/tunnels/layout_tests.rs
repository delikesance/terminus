use super::test_support::{content, item};
use super::*;
use crate::geom::Rect;
#[test]
fn the_description_and_new_button_share_the_header_row() {
    let l = list_layout(content(), 0, &Metrics::default());
    assert!(l.new_button.right() <= content().right() - PAD + 0.01);
    assert!(l.description.right() <= l.new_button.x);
    assert!((l.new_button.y - l.description.y).abs() < 20.0);
    assert!(l.cards.is_empty());
    assert!(l.empty.is_some(), "empty state");
}

#[test]
fn cards_stack_without_overlap_inside_the_content() {
    let l = list_layout(content(), 3, &Metrics::default());
    assert_eq!(l.cards.len(), 3);
    assert!(l.empty.is_none());
    for w in l.cards.windows(2) {
        assert!((w[1].y - w[0].bottom() - CARD_GAP).abs() < 0.01);
    }
    for c in &l.cards {
        assert!(c.x >= content().x && c.right() <= content().right());
    }
    let mut all: Vec<(&'static str, Rect)> =
        l.cards.iter().map(|c| ("card", *c)).collect();
    all.push(("new", l.new_button));
    crate::overlap::assert_no_overlaps(&all, "tunnels list");
}

#[test]
fn hit_testing_finds_new_toggle_delete_and_card() {
    let items = vec![
        item("a", TunnelKind::Local, TunnelStatus::Running),
        item("b", TunnelKind::Local, TunnelStatus::Stopped),
    ];
    let m = Metrics::default();
    let l = list_layout(content(), 2, &m);
    let c = l.new_button;
    assert_eq!(
        list_hit(content(), &items, &m, c.x + 2.0, c.y + 2.0),
        Some(ListHit::New)
    );
    let cl = card_geometry(l.cards[1], &m, items[1].status);
    let a = cl.actions[0].unwrap();
    assert_eq!(
        list_hit(content(), &items, &m, a.x + 2.0, a.y + 2.0),
        Some(ListHit::Toggle(1))
    );
    let t = cl.actions[1].unwrap();
    assert_eq!(
        list_hit(content(), &items, &m, t.x + 2.0, t.y + 2.0),
        Some(ListHit::Delete(1))
    );
    let body = l.cards[0];
    assert_eq!(
        list_hit(content(), &items, &m, body.x + 30.0, body.y + 5.0),
        Some(ListHit::Card(0))
    );
    assert_eq!(list_hit(content(), &items, &m, 1.0, 1.0), None);
}

#[test]
fn the_dialog_sits_inside_the_content_and_dynamic_hides_the_destination() {
    let mut f = TunnelForm::new("h");
    let d = dialog_layout(content(), &f, &Metrics::default());
    assert!(d.dialog.x >= content().x && d.dialog.right() <= content().right());
    assert!(d.dialog.y >= content().y && d.dialog.bottom() <= content().bottom());
    assert!(d.dest.is_some() && d.port.is_some());
    f.set_kind(TunnelKind::Dynamic);
    let d = dialog_layout(content(), &f, &Metrics::default());
    assert!(d.dest.is_none() && d.port.is_none());
    assert!(d.confirm.right() <= d.dialog.right());
    assert!(d.cancel.right() <= d.confirm.x);
}

#[test]
fn dialog_hits_cover_kind_fields_and_buttons() {
    let f = TunnelForm::new("h");
    let m = Metrics::default();
    let d = dialog_layout(content(), &f, &m);
    let at = |r: Rect| (r.x + 3.0, r.y + 3.0);
    let (x, y) = at(d.segments.segments[2]);
    assert_eq!(dialog_hit(&d, &f, x, y), DialogHit::Kind(2));
    let (x, y) = at(d.name.box_rect);
    assert_eq!(dialog_hit(&d, &f, x, y), DialogHit::Field(FormField::Name));
    let (x, y) = at(d.local.box_rect);
    assert_eq!(
        dialog_hit(&d, &f, x, y),
        DialogHit::Field(FormField::LocalPort)
    );
    let (x, y) = at(d.cancel);
    assert_eq!(dialog_hit(&d, &f, x, y), DialogHit::Cancel);
    let (x, y) = at(d.confirm);
    assert_eq!(dialog_hit(&d, &f, x, y), DialogHit::Confirm);
    assert_eq!(dialog_hit(&d, &f, 1.0, 1.0), DialogHit::Outside);
}
