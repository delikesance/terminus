use super::*;
use crate::geom::Rect;

pub(super) fn content() -> Rect {
    Rect::new(260.0, 96.0, 1180.0, 804.0)
}

pub(super) fn item(id: &str, kind: TunnelKind, status: TunnelStatus) -> TunnelItem {
    TunnelItem {
        id: id.into(),
        name: format!("tunnel {id}"),
        kind,
        bind_host: "127.0.0.1".into(),
        bind_port: 5432,
        dest_host: "localhost".into(),
        dest_port: 5432,
        status,
        error: None,
        stats: None,
    }
}

pub(super) fn free(_: u16) -> bool {
    true
}
