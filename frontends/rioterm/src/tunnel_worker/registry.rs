use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::args::friendly_error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TunnelEvent {
    /// The process survived the settle delay.
    Running(String),
    /// The process ended without being asked to.
    Exited { id: String, message: String },
}

struct Proc {
    child: Child,
    started: Instant,
    stderr: Arc<Mutex<String>>,
    reader: Option<std::thread::JoinHandle<()>>,
    reported: bool,
    /// Temp key / askpass files of this ssh, removed when it is reaped.
    _secrets: crate::ssh_secrets::SecretFiles,
}

/// Child `ssh` processes keyed by tunnel id. Dropping the registry kills
/// them all.
pub struct TunnelRegistry {
    procs: HashMap<String, Proc>,
    pub(super) settle: Duration,
}

impl TunnelRegistry {
    pub fn new(settle: Duration) -> Self {
        Self {
            procs: HashMap::new(),
            settle,
        }
    }

    /// Spawn `cmd` for tunnel `id`, replacing any process already there.
    pub fn start(&mut self, id: &str, mut cmd: Command) -> Result<(), String> {
        self.stop(id);
        // Before spawning: a failed spawn drops it and removes the files.
        let secrets =
            crate::ssh_secrets::SecretFiles::new(crate::ssh_secrets::launch_secrets(
                cmd.get_args().filter_map(|a| a.to_str()),
                cmd.get_envs()
                    .filter_map(|(k, v)| Some((k.to_str()?, v?.to_str()?))),
            ));
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        // A killed app (SIGTERM/SIGKILL never run Drop) must not leave
        // `ssh -N` holding its ports: the child gets SIGTERM when the
        // thread that started it (the UI thread) goes away.
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::process::CommandExt;
            // SAFETY: prctl is async-signal-safe; nothing else runs here.
            unsafe {
                cmd.pre_exec(|| {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Could not start ssh: {e}"))?;
        let stderr = Arc::new(Mutex::new(String::new()));
        let reader = child.stderr.take().map(|mut pipe| {
            let sink = stderr.clone();
            std::thread::spawn(move || {
                use std::io::Read;
                let mut buf = [0u8; 1024];
                while let Ok(n) = pipe.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    if let Ok(mut s) = sink.lock() {
                        if s.len() < 8192 {
                            s.push_str(&String::from_utf8_lossy(&buf[..n]));
                        }
                    }
                }
            })
        });
        self.procs.insert(
            id.to_string(),
            Proc {
                child,
                started: Instant::now(),
                stderr,
                reader,
                reported: false,
                _secrets: secrets,
            },
        );
        Ok(())
    }

    /// Kill and reap the process of `id`; false when there was none.
    pub fn stop(&mut self, id: &str) -> bool {
        match self.procs.remove(id) {
            Some(mut p) => {
                let _ = p.child.kill();
                let _ = p.child.wait();
                true
            }
            None => false,
        }
    }

    pub fn stop_all(&mut self) {
        let ids: Vec<String> = self.procs.keys().cloned().collect();
        for id in ids {
            self.stop(&id);
        }
    }

    pub fn is_active(&self, id: &str) -> bool {
        self.procs.contains_key(id)
    }

    #[cfg(test)]
    pub fn active_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.procs.keys().cloned().collect();
        ids.sort();
        ids
    }

    pub fn pid(&self, id: &str) -> Option<u32> {
        self.procs.get(id).map(|p| p.child.id())
    }

    /// How long the process of `id` has been alive.
    pub fn uptime(&self, id: &str) -> Option<Duration> {
        self.procs.get(id).map(|p| p.started.elapsed())
    }

    /// Detect exits and settled tunnels. Call on every UI tick.
    pub fn poll(&mut self) -> Vec<TunnelEvent> {
        let mut events = Vec::new();
        let mut gone = Vec::new();
        for (id, p) in self.procs.iter_mut() {
            match p.child.try_wait() {
                Ok(Some(status)) => {
                    // The reader thread usually has the last line in flight.
                    if let Some(reader) = p.reader.take() {
                        let end = Instant::now() + Duration::from_millis(200);
                        while !reader.is_finished() && Instant::now() < end {
                            std::thread::sleep(Duration::from_millis(2));
                        }
                        if reader.is_finished() {
                            let _ = reader.join();
                        }
                    }
                    let text = p.stderr.lock().map(|s| s.clone()).unwrap_or_default();
                    events.push(TunnelEvent::Exited {
                        id: id.clone(),
                        message: friendly_error(&text, status.code()),
                    });
                    gone.push(id.clone());
                }
                Ok(None) => {
                    if !p.reported && p.started.elapsed() >= self.settle {
                        p.reported = true;
                        events.push(TunnelEvent::Running(id.clone()));
                    }
                }
                Err(err) => {
                    let _ = p.child.kill();
                    let _ = p.child.wait();
                    events.push(TunnelEvent::Exited {
                        id: id.clone(),
                        message: format!("Lost track of the ssh process: {err}"),
                    });
                    gone.push(id.clone());
                }
            }
        }
        for id in gone {
            self.procs.remove(&id);
        }
        events
    }
}

impl Drop for TunnelRegistry {
    fn drop(&mut self) {
        self.stop_all();
    }
}
