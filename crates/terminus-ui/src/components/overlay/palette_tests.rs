use super::*;

fn palette() -> Palette {
    Palette::new(
        "spl",
        vec![PaletteGroup::new(
            "Commands",
            vec![
                PaletteItem::new("Split Right", "Terminal"),
                PaletteItem::new("Split Down", "Terminal"),
            ],
        )],
    )
}

#[test]
fn palette_layout_is_centred_and_sized() {
    let p = palette();
    let l = p.layout((1200.0, 800.0));
    assert_eq!(l.panel.width, PALETTE_WIDTH);
    assert!((l.panel.x + l.panel.width / 2.0 - 600.0).abs() < 0.5);
    assert_eq!(l.query.height, PALETTE_QUERY_HEIGHT);
    assert_eq!(l.rows.len(), 3, "one header + two items");
    assert!(matches!(l.rows[0], PaletteRow::Header { .. }));
    let items: Vec<_> = l
        .rows
        .iter()
        .filter_map(|r| match r {
            PaletteRow::Item { rect, index } => Some((*rect, *index)),
            _ => None,
        })
        .collect();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].0.height, PALETTE_ITEM_HEIGHT);
    assert_eq!(items[1].1, 1);
    assert!((items[1].0.y - items[0].0.bottom() - PALETTE_ITEM_GAP).abs() < 0.01);
    assert!(l.footer.y >= items[1].0.bottom());
    assert!((l.footer.bottom() - l.panel.bottom()).abs() < 0.5);
    assert!(l.empty.is_none());
}

#[test]
fn palette_selection_wraps_and_resolves_choice() {
    let mut p = palette();
    assert_eq!(p.selected, 0);
    p.move_selection(1);
    assert_eq!(p.selected, 1);
    p.move_selection(1);
    assert_eq!(p.selected, 0);
    p.move_selection(-1);
    assert_eq!(p.selected, 1);
    assert_eq!(p.choice(), Some(PaletteChoice::Item(1)));
}

#[test]
fn palette_empty_result_offers_add_server() {
    let p = Palette::new("splt", vec![]);
    assert!(p.is_empty());
    assert_eq!(p.add_server_label(), "Add server \u{201c}splt\u{201d}");
    assert_eq!(p.choice(), Some(PaletteChoice::AddServer("splt".into())));
    let l = p.layout((1200.0, 800.0));
    assert!(l.empty.is_some());
    assert!(l.rows.is_empty());
    let e = l.empty.unwrap();
    assert_eq!(l.hit_test(e.x + 3.0, e.y + 3.0), PaletteHit::AddServer);
    let blank = Palette::new("", vec![]);
    assert_eq!(blank.choice(), None, "nothing to add for an empty query");
}

#[test]
fn palette_hit_test() {
    let p = palette();
    let l = p.layout((1200.0, 800.0));
    let item = l
        .rows
        .iter()
        .find_map(|r| match r {
            PaletteRow::Item { rect, index: 1 } => Some(*rect),
            _ => None,
        })
        .unwrap();
    assert_eq!(l.hit_test(item.x + 5.0, item.y + 5.0), PaletteHit::Item(1));
    assert_eq!(
        l.hit_test(l.query.x + 5.0, l.query.y + 5.0),
        PaletteHit::Inside
    );
    assert_eq!(l.hit_test(1.0, 1.0), PaletteHit::Outside);
}

#[test]
fn palette_keys() {
    assert_eq!(palette_key(PaletteKey::Escape), PaletteAction::Close);
    assert_eq!(palette_key(PaletteKey::Enter), PaletteAction::Run);
    assert_eq!(palette_key(PaletteKey::Down), PaletteAction::Move(1));
    assert_eq!(palette_key(PaletteKey::Up), PaletteAction::Move(-1));
}

#[test]
fn palette_footer_hints() {
    assert_eq!(
        PALETTE_HINTS,
        ["Enter to run", "Arrows to move", "Esc to close"]
    );
}
