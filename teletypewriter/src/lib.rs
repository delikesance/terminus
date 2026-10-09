extern crate libc;

#[cfg(not(windows))]
mod unix;
#[cfg(not(windows))]
pub use self::unix::*;

#[cfg(windows)]
pub mod windows;
#[cfg(windows)]
pub use self::windows::*;

use std::io;

#[repr(C)]
pub struct Winsize {
    ws_row: libc::c_ushort,
    ws_col: libc::c_ushort,
    ws_xpixel: libc::c_ushort,
    ws_ypixel: libc::c_ushort,
}

pub trait ProcessReadWrite {
    type Reader: io::Read;
    type Writer: io::Write;
    fn reader(&mut self) -> &mut Self::Reader;
    fn read_token(&self) -> corcovado::Token;
    fn writer(&mut self) -> &mut Self::Writer;
    fn write_token(&self) -> corcovado::Token;
    fn set_winsize(&mut self, _: WinsizeBuilder) -> Result<(), io::Error>;

    fn register(
        &mut self,
        _: &corcovado::Poll,
        _: &mut dyn Iterator<Item = corcovado::Token>,
        _: corcovado::Ready,
        _: corcovado::PollOpt,
    ) -> io::Result<()>;
    fn reregister(
        &mut self,
        _: &corcovado::Poll,
        _: corcovado::Ready,
        _: corcovado::PollOpt,
    ) -> io::Result<()>;
    fn deregister(&mut self, _: &corcovado::Poll) -> io::Result<()>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum ChildEvent {
    /// Indicates the child has exited, with the raw wait status when the
    /// platform makes it available (interpret with
    /// `std::process::ExitStatus::from_raw` / `ExitStatusExt`).
    Exited(Option<i32>),
}

pub trait EventedPty: ProcessReadWrite {
    fn child_event_token(&self) -> corcovado::Token;

    /// Tries to retrieve an event.
    ///
    /// Returns `Some(event)` on success, or `None` if there are no events to retrieve.
    fn next_child_event(&mut self) -> Option<ChildEvent>;
}

#[derive(Debug, Clone)]
pub struct WinsizeBuilder {
    pub rows: u16,
    pub cols: u16,
    pub width: u16,
    pub height: u16,
}

impl WinsizeBuilder {
    fn build(&self) -> Winsize {
        let ws_row = self.rows as libc::c_ushort;
        let ws_col = self.cols as libc::c_ushort;
        let ws_xpixel = self.width as libc::c_ushort;
        let ws_ypixel = self.height as libc::c_ushort;

        Winsize {
            ws_row,
            ws_col,
            ws_xpixel,
            ws_ypixel,
        }
    }
}

/// Variables a Windows child needs to keep truecolor.
///
/// `COLORTERM=truecolor` is what apps read to skip 256-color quantization.
/// A `wsl.exe` child only receives Windows variables listed in `WSLENV`, so
/// without that entry a Linux program inside WSL never sees `COLORTERM` and
/// downgrades its colors, while the same program on a Linux host looks right.
/// Pure so it can be tested off Windows; `wslenv` is the inherited value.
pub fn windows_color_env(wslenv: Option<&str>) -> Vec<(String, String)> {
    let existing = wslenv.unwrap_or("");
    let listed = existing
        .split(':')
        .any(|entry| entry.split('/').next() == Some("COLORTERM"));
    let wslenv = match (listed, existing.is_empty()) {
        (true, _) => existing.to_string(),
        (false, true) => "COLORTERM".to_string(),
        (false, false) => format!("{existing}:COLORTERM"),
    };
    vec![
        ("COLORTERM".to_string(), "truecolor".to_string()),
        ("WSLENV".to_string(), wslenv),
    ]
}

#[cfg(test)]
mod color_env_tests {
    use super::windows_color_env;

    fn get<'a>(env: &'a [(String, String)], key: &str) -> Option<&'a str> {
        env.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    #[test]
    fn truecolor_is_advertised_and_forwarded_into_wsl() {
        let env = windows_color_env(None);
        assert_eq!(get(&env, "COLORTERM"), Some("truecolor"));
        assert_eq!(get(&env, "WSLENV"), Some("COLORTERM"));
    }

    #[test]
    fn existing_wslenv_entries_are_kept() {
        let env = windows_color_env(Some("PATH/l:FOO"));
        assert_eq!(get(&env, "WSLENV"), Some("PATH/l:FOO:COLORTERM"));
    }

    #[test]
    fn colorterm_is_not_listed_twice() {
        let env = windows_color_env(Some("A:COLORTERM/u"));
        assert_eq!(get(&env, "WSLENV"), Some("A:COLORTERM/u"));
    }
}
