use crate::error::{Error, Result};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
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
        output: mpsc::UnboundedSender<Vec<u8>>,
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
                        if output.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
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
        let _ = self.child.lock().kill();
        Ok(())
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
    let handle = thread::spawn(move || {
        let mut out = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => out.extend_from_slice(&buf[..n]),
                Err(_) => break,
            }
            if started.elapsed() > timeout {
                break;
            }
        }
        out
    });
    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait().ok().flatten().is_some() {
            break;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let _ = child.wait();
    handle
        .join()
        .map_err(|_| Error::msg("pty reader thread panicked"))
}
