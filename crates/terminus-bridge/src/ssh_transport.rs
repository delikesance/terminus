//! A remote shell exposed as a Rio PTY.
//!
//! [`SshTransport`] implements [`teletypewriter::EventedPty`], so an SSH
//! session feeds the very same `Machine → Crosswords → Sugarloaf` pipeline as a
//! local PTY/ConPTY: nothing in the renderer or the VT parser needs to know the
//! bytes came from the network.
//!
//! **Status:** library + tests are ready, but rioterm does **not** open host
//! tabs through this type yet. Interactive sessions use system `ssh` in a local
//! PTY (`screen::ssh_shell`, milestone Option A). Wiring this transport into
//! `SessionSpec::Ssh` is roadmap **1.4-debt**.
//!
//! corcovado (Rio's poll loop: a thread with a mio fork) and russh (tokio)
//! cannot share a reactor, so the two halves are kept strictly apart and talk
//! through this module:
//!
//! * [`SshTransport`] is the corcovado half: a byte buffer plus three readiness
//!   registrations (readable / writable / child-exit), handed to
//!   `Poll::register` exactly like `teletypewriter::Pty` does.
//! * [`SshPump`] is the tokio half: it owns the [`SshSession`], drains remote
//!   channel events into the buffer and executes the commands the Rio event loop
//!   sends (keystrokes, resizes, shutdown).
//!
//! Readiness is level-triggered, matching the registration the PTY loop uses:
//! the read registration is cleared as soon as the reader drains the buffer and
//! raised again when the pump pushes more bytes, and the write registration is
//! only armed while the loop actually wants to write. That combination is what
//! keeps a poll from spinning on an always-ready pipe.

use std::collections::VecDeque;
use std::io::{self, ErrorKind, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use corcovado::event::Evented;
use corcovado::{Poll, PollOpt, Ready, Registration, SetReadiness, Token};
use teletypewriter::{ChildEvent, EventedPty, ProcessReadWrite, WinsizeBuilder};
use terminus_core::ssh::{SshEvent, SshPty, SshSession};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::sync::Notify;
use tracing::{debug, info, warn};

/// Bytes the pump may buffer ahead of the terminal before it stops reading the
/// remote channel (~1 MiB, roughly one second of a flooding command).
///
/// Without this the event loop could be starved: a `yes` on the remote host
/// would fill memory faster than the renderer drains glyphs.
pub const MAX_PENDING_BYTES: usize = 1 << 20;

/// What the corcovado half asks the tokio half to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Keystrokes typed in the terminal.
    Write(Vec<u8>),
    /// New window size (`Msg::Resize`).
    Resize {
        /// Columns.
        cols: u32,
        /// Rows.
        rows: u32,
    },
    /// The Rio side dropped the pane: close the remote session.
    Shutdown,
}

/// Lifecycle of the remote session, as seen by the Rio event loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportState {
    /// Connected and pumping.
    Running,
    /// The remote shell exited with this status (`None` when unknown).
    Exited(Option<i32>),
    /// The session died on an error; the message is for the log/UI.
    Failed(String),
}

impl TransportState {
    /// Whether this state means the pane is over.
    pub const fn is_finished(&self) -> bool {
        !matches!(self, TransportState::Running)
    }
}

/// State shared by the corcovado half and the tokio half.
struct Inner {
    /// Bytes read from the remote channel, waiting for the terminal to parse.
    out: Mutex<VecDeque<u8>>,
    /// Signalled by the reader when it drained `out`, so the pump resumes.
    space: Notify,
    /// Session lifecycle.
    state: Mutex<TransportState>,
    /// `true` once the exit status was handed to the event loop.
    exit_reported: AtomicBool,
    /// `true` once no more bytes will ever arrive.
    eof: AtomicBool,
    /// `true` once the Rio side dropped the transport.
    closed: AtomicBool,
    /// Commands mailbox (write / resize / shutdown).
    commands: Mutex<Option<UnboundedSender<Command>>>,
    read_reg: Registration,
    read_ready: SetReadiness,
    write_reg: Registration,
    write_ready: SetReadiness,
    child_reg: Registration,
    child_ready: SetReadiness,
}

impl Inner {
    fn pending(&self) -> usize {
        self.out.lock().expect("transport buffer").len()
    }

    /// Raises the read readiness so the poll reports the pane readable.
    fn arm_readable(&self) {
        if !self.read_ready.readiness().is_readable() {
            let _ = self.read_ready.set_readiness(Ready::readable());
        }
    }

    /// Clears the read readiness: nothing left to read.
    fn disarm_readable(&self) {
        if self.read_ready.readiness().is_readable() {
            let _ = self.read_ready.set_readiness(Ready::empty());
        }
    }

    /// Queues bytes for the terminal, returning how many were accepted.
    fn push(&self, data: &[u8]) -> usize {
        if data.is_empty() {
            return 0;
        }
        {
            let mut out = self.out.lock().expect("transport buffer");
            out.extend(data.iter().copied());
        }
        self.arm_readable();
        data.len()
    }

    /// Moves the session to a terminal state and wakes the event loop.
    fn finish(&self, state: TransportState) {
        {
            let mut current = self.state.lock().expect("transport state");
            if current.is_finished() {
                return;
            }
            *current = state;
        }
        self.eof.store(true, Ordering::SeqCst);
        self.arm_readable();
        let _ = self.child_ready.set_readiness(Ready::readable());
    }

    fn send(&self, command: Command) -> bool {
        let sender = self.commands.lock().expect("transport commands").clone();
        match sender {
            Some(tx) => tx.send(command).is_ok(),
            None => false,
        }
    }
}

/// The corcovado-side handle: what `Machine<T: EventedPty>` receives.
pub struct SshTransport {
    inner: Arc<Inner>,
    read_token: Token,
    write_token: Token,
    child_token: Token,
}

/// The tokio-side handle: drives the [`SshSession`] and feeds the transport.
pub struct SshPump {
    inner: Arc<Inner>,
    commands: UnboundedReceiver<Command>,
}

impl SshPump {
    /// Bytes buffered and not yet read by the terminal.
    pub fn pending_bytes(&self) -> usize {
        self.inner.pending()
    }

    /// The session lifecycle as of now.
    pub fn state(&self) -> TransportState {
        self.inner.state.lock().expect("transport state").clone()
    }

    /// Queues remote bytes for the terminal.
    pub fn push_data(&self, data: &[u8]) -> usize {
        self.inner.push(data)
    }

    /// Writes remote stderr into the same stream the terminal parses.
    pub fn push_extended(&self, data: &[u8]) -> usize {
        self.inner.push(data)
    }

    /// Waits until the terminal caught up, if the buffer is over its budget.
    pub async fn wait_for_space(&self) {
        while self.inner.pending() >= MAX_PENDING_BYTES {
            let notified = self.inner.space.notified();
            if self.inner.pending() < MAX_PENDING_BYTES {
                return;
            }
            notified.await;
        }
    }

    /// Marks the remote side as having sent everything it will send.
    pub fn mark_eof(&self) {
        self.inner.eof.store(true, Ordering::SeqCst);
        self.inner.arm_readable();
    }

    /// Records a clean remote exit.
    pub fn exit(&self, status: Option<i32>) {
        self.inner.finish(TransportState::Exited(status));
    }

    /// Records a fatal transport error.
    pub fn fail(&self, message: impl Into<String>) {
        let message = message.into();
        warn!(error = %message, "ssh transport failed");
        self.inner.finish(TransportState::Failed(message));
    }

    /// Records the exit only when the session is still running.
    pub fn exit_if_running(&self, status: Option<i32>) {
        if !self.state().is_finished() {
            self.exit(status);
        }
    }

    /// Whether the Rio side asked the session to stop.
    pub fn is_shutdown(&self) -> bool {
        self.inner.closed.load(Ordering::SeqCst)
    }

    /// Next command from the Rio event loop; `None` once the pane is gone.
    pub async fn recv_command(&mut self) -> Option<Command> {
        self.commands.recv().await
    }
}

/// Creates a transport and its pump.
pub fn pair() -> (SshTransport, SshPump) {
    let (tx, rx) = mpsc::unbounded_channel();
    let (read_reg, read_ready) = Registration::new2();
    let (write_reg, write_ready) = Registration::new2();
    let (child_reg, child_ready) = Registration::new2();

    let inner = Arc::new(Inner {
        out: Mutex::new(VecDeque::new()),
        space: Notify::new(),
        state: Mutex::new(TransportState::Running),
        exit_reported: AtomicBool::new(false),
        eof: AtomicBool::new(false),
        closed: AtomicBool::new(false),
        commands: Mutex::new(Some(tx)),
        read_reg,
        read_ready,
        write_reg,
        write_ready,
        child_reg,
        child_ready,
    });

    (
        SshTransport {
            inner: Arc::clone(&inner),
            read_token: Token(0),
            write_token: Token(0),
            child_token: Token(0),
        },
        SshPump {
            inner,
            commands: rx,
        },
    )
}

impl SshTransport {
    /// Lifecycle of the remote session.
    pub fn state(&self) -> TransportState {
        self.inner.state.lock().expect("transport state").clone()
    }

    /// Asks the remote side to close without waiting (tab close, shutdown).
    pub fn shutdown(&self) {
        self.inner.closed.store(true, Ordering::SeqCst);
        self.inner.send(Command::Shutdown);
    }
}

impl Drop for SshTransport {
    fn drop(&mut self) {
        // Dropping the pane must not leave a remote shell running.
        self.shutdown();
    }
}

impl Read for SshTransport {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let mut out = self.inner.out.lock().expect("transport buffer");
        if out.is_empty() {
            // Nothing buffered: either the session is over (EOF, the parser
            // stops on `Ok(0)`) or the poll will wake us when bytes land.
            if self.inner.eof.load(Ordering::SeqCst) {
                self.inner.disarm_readable();
                return Ok(0);
            }
            return Err(io::Error::new(ErrorKind::WouldBlock, "no ssh data yet"));
        }

        let take = buf.len().min(out.len());
        for slot in buf.iter_mut().take(take) {
            *slot = out.pop_front().expect("length checked");
        }
        let drained = out.is_empty();
        drop(out);

        if drained {
            // Level-triggered: clear the registration so the next poll blocks
            // instead of spinning on an empty buffer.
            self.inner.disarm_readable();
            self.inner.space.notify_one();
        }

        Ok(take)
    }
}

impl Write for SshTransport {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if !self.inner.send(Command::Write(buf.to_vec())) {
            return Err(io::Error::new(ErrorKind::BrokenPipe, "ssh session is gone"));
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl ProcessReadWrite for SshTransport {
    // The transport is its own reader and writer: reads pop from the pump
    // buffer, writes are queued as commands for the pump task.
    type Reader = Self;
    type Writer = Self;

    fn reader(&mut self) -> &mut Self::Reader {
        self
    }

    fn writer(&mut self) -> &mut Self::Writer {
        self
    }

    fn read_token(&self) -> Token {
        self.read_token
    }

    fn write_token(&self) -> Token {
        self.write_token
    }

    fn set_winsize(&mut self, winsize: WinsizeBuilder) -> Result<(), io::Error> {
        let cols = u32::from(winsize.cols);
        let rows = u32::from(winsize.rows);
        debug!(cols, rows, "ssh: window change");
        if !self.inner.send(Command::Resize { cols, rows }) {
            return Err(io::Error::new(ErrorKind::BrokenPipe, "ssh session is gone"));
        }
        Ok(())
    }

    fn register(
        &mut self,
        poll: &Poll,
        tokens: &mut dyn Iterator<Item = Token>,
        interest: Ready,
        poll_opts: PollOpt,
    ) -> io::Result<()> {
        self.read_token = tokens.next().expect("read token");
        self.write_token = tokens.next().expect("write token");
        self.child_token = tokens.next().expect("child token");

        self.inner.read_reg.register(
            poll,
            self.read_token,
            if interest.is_readable() {
                Ready::readable()
            } else {
                Ready::empty()
            },
            poll_opts,
        )?;

        self.inner.write_reg.register(
            poll,
            self.write_token,
            if interest.is_writable() {
                Ready::writable()
            } else {
                Ready::empty()
            },
            poll_opts,
        )?;

        // Child events (session over) are always interesting.
        self.inner.child_reg.register(
            poll,
            self.child_token,
            Ready::readable(),
            poll_opts,
        )?;

        // Writes are queued, never blocking: as soon as the event loop asks for
        // write interest the registration can fire.
        let _ = self.inner.write_ready.set_readiness(Ready::writable());

        Ok(())
    }

    fn reregister(
        &mut self,
        poll: &Poll,
        interest: Ready,
        poll_opts: PollOpt,
    ) -> io::Result<()> {
        self.inner.read_reg.reregister(
            poll,
            self.read_token,
            if interest.is_readable() {
                Ready::readable()
            } else {
                Ready::empty()
            },
            poll_opts,
        )?;

        self.inner.write_reg.reregister(
            poll,
            self.write_token,
            if interest.is_writable() {
                Ready::writable()
            } else {
                Ready::empty()
            },
            poll_opts,
        )?;

        Ok(())
    }

    fn deregister(&mut self, poll: &Poll) -> io::Result<()> {
        self.inner.read_reg.deregister(poll)?;
        self.inner.write_reg.deregister(poll)?;
        self.inner.child_reg.deregister(poll)
    }
}

impl EventedPty for SshTransport {
    fn child_event_token(&self) -> Token {
        self.child_token
    }

    fn next_child_event(&mut self) -> Option<ChildEvent> {
        let state = self.state();
        if !state.is_finished() {
            return None;
        }
        if self.inner.exit_reported.swap(true, Ordering::SeqCst) {
            return None;
        }
        let _ = self.inner.child_ready.set_readiness(Ready::empty());

        match state {
            TransportState::Exited(status) => {
                info!(status = ?status, "ssh: remote session exited");
                Some(ChildEvent::Exited(status))
            }
            TransportState::Failed(message) => {
                warn!(error = %message, "ssh: remote session failed");
                Some(ChildEvent::Exited(None))
            }
            TransportState::Running => None,
        }
    }
}

/// Drives `session` until it dies, feeding `pump` and executing its commands.
///
/// Runs on the bridge's tokio runtime; returns once the remote shell exited or
/// the Rio side dropped the pane.
pub async fn run_session(mut session: SshSession, mut pump: SshPump, pty: SshPty) {
    if let Err(err) = session.open_shell(pty.clone()).await {
        pump.fail(err.to_string());
        return;
    }
    info!(target_host = %session.target(), cols = pty.cols, rows = pty.rows, "ssh: shell started");

    loop {
        tokio::select! {
            command = pump.recv_command() => match command {
                None | Some(Command::Shutdown) => break,
                Some(Command::Write(bytes)) => {
                    if let Err(err) = session.write(&bytes).await {
                        pump.fail(err.to_string());
                        break;
                    }
                }
                Some(Command::Resize { cols, rows }) => {
                    if let Err(err) = session.resize(cols, rows).await {
                        warn!(error = %err, "ssh: window change failed");
                    }
                }
            },
            event = session.next_event() => match event {
                None => break,
                Some(SshEvent::Data(bytes)) => {
                    pump.push_data(&bytes);
                    pump.wait_for_space().await;
                }
                Some(SshEvent::ExtendedData { data, .. }) => {
                    pump.push_extended(&data);
                    pump.wait_for_space().await;
                }
                Some(SshEvent::Eof) => pump.mark_eof(),
                Some(SshEvent::ExitStatus(status)) => {
                    pump.exit(Some(status as i32));
                    break;
                }
                Some(SshEvent::ExitSignal { signal }) => {
                    warn!(signal = %signal, "ssh: remote shell killed by signal");
                    pump.exit(Some(1));
                    break;
                }
                Some(SshEvent::Closed) => break,
            },
        }
    }

    let _ = session.disconnect().await;
    // Whatever ended the loop, the pane must not hang waiting for bytes.
    pump.mark_eof();
    pump.exit_if_running(None);
    debug!(target_host = %session.target(), "ssh: session finished");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn register_pty(transport: &mut SshTransport, poll: &Poll) {
        let mut tokens = (0..).map(Token);
        transport
            .register(poll, &mut tokens, Ready::readable(), PollOpt::level())
            .expect("register");
    }

    #[test]
    fn buffered_bytes_are_read_then_readiness_clears() {
        let (mut transport, pump) = pair();
        let poll = Poll::new().expect("poll");
        register_pty(&mut transport, &poll);

        assert_eq!(pump.push_data(b"hello"), 5);

        let mut events = corcovado::Events::with_capacity(8);
        poll.poll(&mut events, Some(Duration::from_millis(200)))
            .expect("poll");
        let event = events
            .iter()
            .find(|e| e.token() == transport.read_token())
            .expect("readable event");
        assert!(event.readiness().is_readable());

        let mut buf = [0u8; 16];
        let n = transport.read(&mut buf).expect("read");
        assert_eq!(&buf[..n], b"hello");

        // Drained: the registration must go quiet, otherwise the poll spins.
        assert!(!transport.inner.read_ready.readiness().is_readable());
        assert!(matches!(
            transport.read(&mut buf).unwrap_err().kind(),
            ErrorKind::WouldBlock
        ));

        // New bytes re-arm it.
        pump.push_data(b"world");
        events.clear();
        poll.poll(&mut events, Some(Duration::from_millis(200)))
            .expect("poll");
        assert!(events
            .iter()
            .any(|e| e.token() == transport.read_token() && e.readiness().is_readable()));
    }

    #[test]
    fn write_interest_is_only_armed_when_requested() {
        let (mut transport, mut pump) = pair();
        let poll = Poll::new().expect("poll");
        register_pty(&mut transport, &poll);

        // Registered readable-only: no writable event must arrive.
        let mut events = corcovado::Events::with_capacity(8);
        poll.poll(&mut events, Some(Duration::from_millis(50)))
            .expect("poll");
        assert!(!events.iter().any(|e| e.token() == transport.write_token()));

        transport
            .reregister(
                &poll,
                Ready::readable() | Ready::writable(),
                PollOpt::level(),
            )
            .expect("reregister");
        events.clear();
        poll.poll(&mut events, Some(Duration::from_millis(200)))
            .expect("poll");
        assert!(
            events
                .iter()
                .any(|e| e.token() == transport.write_token()
                    && e.readiness().is_writable())
        );

        // The event loop writes, then drops the interest again.
        let written = transport.write(b"\r").expect("write");
        assert_eq!(written, 1);
        let command = pump
            .commands
            .try_recv()
            .expect("queued command reaches the pump");
        assert_eq!(command, Command::Write(b"\r".to_vec()));
    }

    #[test]
    fn window_change_reaches_the_pump() {
        let (mut transport, mut pump) = pair();
        transport
            .set_winsize(WinsizeBuilder {
                rows: 40,
                cols: 120,
                width: 0,
                height: 0,
            })
            .expect("winsize");

        assert_eq!(
            pump.commands.try_recv().expect("resize command"),
            Command::Resize {
                cols: 120,
                rows: 40
            }
        );
    }

    #[test]
    fn eof_and_exit_status_reach_the_event_loop_once() {
        let (mut transport, pump) = pair();
        let poll = Poll::new().expect("poll");
        register_pty(&mut transport, &poll);

        assert!(transport.next_child_event().is_none());

        pump.push_data(b"bye");
        pump.mark_eof();
        pump.exit(Some(0));

        let mut buf = [0u8; 8];
        assert_eq!(transport.read(&mut buf).expect("read"), 3);
        // EOF after the drain: the parser stops on Ok(0).
        assert_eq!(transport.read(&mut buf).expect("read"), 0);

        let mut events = corcovado::Events::with_capacity(8);
        poll.poll(&mut events, Some(Duration::from_millis(200)))
            .expect("poll");
        assert!(events
            .iter()
            .any(|e| e.token() == transport.child_token && e.readiness().is_readable()));

        assert_eq!(
            transport.next_child_event(),
            Some(ChildEvent::Exited(Some(0)))
        );
        // Reported exactly once: the event loop closes the pane on the first.
        assert!(transport.next_child_event().is_none());
    }

    #[test]
    fn failures_surface_as_an_exit_event() {
        let (mut transport, pump) = pair();
        pump.fail("connection reset");

        assert_eq!(
            transport.state(),
            TransportState::Failed("connection reset".to_string())
        );
        assert_eq!(transport.next_child_event(), Some(ChildEvent::Exited(None)));
    }

    #[test]
    fn dropping_the_pane_shuts_the_session_down() {
        let (transport, mut pump) = pair();
        let state = pump.state();
        assert_eq!(state, TransportState::Running);

        drop(transport);

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match pump.commands.try_recv() {
                Ok(Command::Shutdown) => break,
                Ok(other) => panic!("unexpected command {other:?}"),
                Err(mpsc::error::TryRecvError::Empty) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(err) => panic!("no shutdown command: {err}"),
            }
        }
    }

    #[tokio::test]
    async fn pump_waits_for_the_terminal_to_catch_up() {
        let (mut transport, mut pump) = pair();
        let chunk = vec![b'x'; 64 * 1024];

        // Fill past the budget: the pump must not be able to push more until
        // the reader drains.
        while pump.pending_bytes() < MAX_PENDING_BYTES {
            pump.push_data(&chunk);
        }

        assert!(
            tokio::time::timeout(Duration::from_millis(50), pump.wait_for_space())
                .await
                .is_err(),
            "pump should still be blocked"
        );

        let mut buf = vec![0u8; MAX_PENDING_BYTES + chunk.len()];
        let mut drained = 0;
        while drained < buf.len() {
            match transport.read(&mut buf[drained..]) {
                Ok(0) => break,
                Ok(n) => drained += n,
                Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(err) => panic!("read failed: {err}"),
            }
        }
        assert!(drained >= MAX_PENDING_BYTES);

        tokio::time::timeout(Duration::from_secs(2), pump.wait_for_space())
            .await
            .expect("pump resumes once the terminal caught up");
    }

    #[test]
    fn transport_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<SshTransport>();
    }
}
