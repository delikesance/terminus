use crate::error::{Error, Result};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::io::{self, Read, Write};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

pub struct LocalPty {
    writer: parking_lot::Mutex<Box<dyn Write + Send>>,
    master: parking_lot::Mutex<Box<dyn MasterPty + Send>>,
    child: parking_lot::Mutex<Box<dyn portable_pty::Child + Send + Sync>>,
}

impl LocalPty {
    pub fn spawn(
        cols: u16,
        rows: u16,
        output: mpsc::UnboundedSender<Result<Vec<u8>>>,
    ) -> Result<Arc<Self>> {
        let pair = native_pty_system().openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let mut cmd = CommandBuilder::new_default_prog();
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        if let Some(home) = dirs::home_dir() {
            cmd.cwd(home);
        }
        let child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let session = Arc::new(Self {
            writer: parking_lot::Mutex::new(writer),
            master: parking_lot::Mutex::new(pair.master),
            child: parking_lot::Mutex::new(child),
        });
        thread::spawn(move || {
            let mut buf = [0u8; 65536];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if output.send(Ok(buf[..n].to_vec())).is_err() {
                            break;
                        }
                    }
                    Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                    Err(err) => match classify_pty_read_error(&err) {
                        None => break,
                        Some(pty_err) => {
                            let _ = output.send(Err(pty_err));
                            break;
                        }
                    },
                }
            }
        });
        Ok(session)
    }

    pub fn write(&self, data: &[u8]) -> Result<()> {
        let mut writer = self.writer.lock();
        writer.write_all(data)?;
        if data.len() < 64 {
            writer.flush()?;
        }
        Ok(())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.master
            .lock()
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(Error::from)
    }

    pub fn kill(&self) -> Result<()> {
        map_pty_kill_result(self.child.lock().kill())
    }
}

pub fn run_local_command(program: &str, args: &[&str], timeout: Duration) -> Result<Vec<u8>> {
    let pair = native_pty_system().openpty(PtySize {
        rows: 24,
        cols: 120,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let mut cmd = CommandBuilder::new(program);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.env("TERM", "xterm-256color");
    let mut child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader()?;
    let started = Instant::now();
    let handle = thread::spawn(move || -> Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => out.extend_from_slice(&buf[..n]),
                Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
                Err(err) => match classify_pty_read_error(&err) {
                    None => break,
                    Some(pty_err) => return Err(pty_err),
                },
            }
            if started.elapsed() > timeout {
                break;
            }
        }
        Ok(out)
    });
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {}
            Err(err) => {
                return Err(Error::PtyReader(format!("wait poll failed: {err}")));
            }
        }
        if Instant::now() > deadline {
            map_pty_kill_result(child.kill())?;
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    child
        .wait()
        .map_err(|err| Error::PtyKill(format!("wait failed: {err}")))?;
    handle
        .join()
        .map_err(|_| Error::msg("pty reader thread panicked"))?
}

/// Classify a PTY read error.
///
/// Returns `None` for hangup-style conditions that are equivalent to EOF on a
/// master PTY (notably Linux `EIO` after the slave closes). Returns `Some` for
/// real I/O failures that must surface to the session layer.
fn classify_pty_read_error(err: &io::Error) -> Option<Error> {
    match err.kind() {
        io::ErrorKind::BrokenPipe | io::ErrorKind::UnexpectedEof => None,
        // Linux master PTY returns EIO once the slave side has hung up.
        _ if err.raw_os_error() == Some(libc_eio()) => None,
        _ => Some(Error::PtyReader(err.to_string())),
    }
}

fn libc_eio() -> i32 {
    // EIO is 5 on Linux/macOS; keep this literal to avoid a libc dependency.
    5
}

fn map_pty_kill_result(result: io::Result<()>) -> Result<()> {
    match result {
        Ok(()) => Ok(()),
        // Process already gone — kill's goal is satisfied.
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) if err.raw_os_error() == Some(ESRCH) => Ok(()),
        Err(err) => Err(Error::PtyKill(err.to_string())),
    }
}

const ESRCH: i32 = 3;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, ErrorKind};

    #[test]
    fn benign_pty_hangup_is_eof_not_error() {
        let eio = io::Error::from_raw_os_error(5);
        assert!(classify_pty_read_error(&eio).is_none());

        let broken = io::Error::new(ErrorKind::BrokenPipe, "broken");
        assert!(classify_pty_read_error(&broken).is_none());

        let unexpected = io::Error::new(ErrorKind::UnexpectedEof, "eof");
        assert!(classify_pty_read_error(&unexpected).is_none());
    }

    #[test]
    fn real_pty_read_error_is_typed() {
        let err = io::Error::new(ErrorKind::PermissionDenied, "denied");
        match classify_pty_read_error(&err) {
            Some(Error::PtyReader(msg)) => assert!(msg.contains("denied")),
            other => panic!("expected PtyReader, got {other:?}"),
        }
    }

    #[test]
    fn kill_maps_failure_to_pty_kill() {
        let err = io::Error::new(ErrorKind::PermissionDenied, "cannot signal");
        match map_pty_kill_result(Err(err)) {
            Err(Error::PtyKill(msg)) => assert!(msg.contains("cannot signal")),
            other => panic!("expected PtyKill, got {other:?}"),
        }
    }

    #[test]
    fn kill_treats_already_dead_as_ok() {
        assert!(map_pty_kill_result(Err(io::Error::from_raw_os_error(ESRCH))).is_ok());
        assert!(map_pty_kill_result(Err(io::Error::new(ErrorKind::NotFound, "gone"))).is_ok());
        assert!(map_pty_kill_result(Ok(())).is_ok());
    }

    #[test]
    fn local_pty_kill_propagates_ok() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let pty = LocalPty::spawn(80, 24, tx).expect("spawn local pty");
        pty.kill().expect("kill should succeed");
    }

    #[test]
    fn run_local_command_returns_output() {
        let out = run_local_command("printf", &["hello-pty"], Duration::from_secs(2))
            .expect("printf via pty");
        assert!(
            String::from_utf8_lossy(&out).contains("hello-pty"),
            "output was {:?}",
            String::from_utf8_lossy(&out)
        );
    }

    #[test]
    fn run_local_command_timeout_kill_does_not_swallow() {
        // Sleep longer than the timeout; kill path must return Result (Ok with
        // partial output), not panic or silently ignore kill failures.
        let result = run_local_command("sleep", &["5"], Duration::from_millis(80));
        assert!(
            result.is_ok(),
            "timeout kill should succeed, got {result:?}"
        );
    }
}
