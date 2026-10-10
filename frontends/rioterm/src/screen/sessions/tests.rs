use super::probe::{exit_code, exit_signal, pane_chrome_rect, session_end};
use terminus_ui::lost_session::SessionEnd;

#[test]
#[cfg(unix)]
fn exit_code_decodes_the_raw_wait_status() {
    // waitpid's status: the code sits in the second byte.
    assert_eq!(exit_code(Some(255 << 8)), Some(255));
    assert_eq!(exit_code(Some(0)), Some(0));
    // Killed by SIGHUP: no exit code, so never "connection lost".
    assert_eq!(exit_code(Some(libc::SIGHUP)), None);
    assert_eq!(exit_code(None), None);
}

#[test]
#[cfg(unix)]
fn exit_signal_decodes_the_raw_wait_status() {
    assert_eq!(exit_signal(Some(libc::SIGKILL)), Some(9));
    assert_eq!(exit_signal(Some(libc::SIGHUP)), Some(1));
    // A normal exit has a code, not a signal.
    assert_eq!(exit_signal(Some(1 << 8)), None);
    assert_eq!(exit_signal(Some(0)), None);
    assert_eq!(exit_signal(None), None);
}

#[test]
#[cfg(unix)]
fn session_end_classifies_raw_wait_statuses() {
    assert_eq!(session_end(Some(0)), SessionEnd::Clean);
    assert_eq!(session_end(Some(255 << 8)), SessionEnd::ConnectionLost);
    assert_eq!(session_end(Some(1 << 8)), SessionEnd::Exited(1));
    assert_eq!(session_end(Some(libc::SIGKILL)), SessionEnd::Signaled(9));
    assert_eq!(session_end(None), SessionEnd::Unknown);
}

#[test]
fn pane_rect_is_the_layout_rect_offset_by_the_margin_and_unscaled() {
    // Physical layout rect [x, y, w, h] with a 20x10 physical margin at 2x.
    let rect = pane_chrome_rect([100.0, 40.0, 600.0, 300.0], 20.0, 10.0, 2.0);
    assert_eq!(
        (rect.x, rect.y, rect.width, rect.height),
        (60.0, 25.0, 300.0, 150.0)
    );
}
