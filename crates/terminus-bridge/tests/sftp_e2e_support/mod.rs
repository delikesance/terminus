//! Shared helpers for the SFTP E2E suite.

pub mod mock_server;

use std::path::PathBuf;
use std::time::Duration;

use terminus_bridge::{SftpCommand, SftpEvent, SftpListEntry, SftpSide, SftpWorker};

pub struct SessionDriver {
    pub worker: SftpWorker,
}

impl SessionDriver {
    pub fn local_dual(left: PathBuf, right: PathBuf) -> Self {
        let worker = SftpWorker::spawn(None);
        worker.send(SftpCommand::ListLocal {
            side: SftpSide::Left,
            path: left,
        });
        worker.send(SftpCommand::ListLocal {
            side: SftpSide::Right,
            path: right,
        });
        Self { worker }
    }

    pub fn list_local(&self, side: SftpSide, path: PathBuf) {
        self.worker.send(SftpCommand::ListLocal { side, path });
    }

    pub fn mkdir_local(&self, side: SftpSide, path: PathBuf) {
        self.worker.send(SftpCommand::MkdirLocal { side, path });
    }

    pub fn rename_local(&self, side: SftpSide, from: PathBuf, to: PathBuf) {
        self.worker
            .send(SftpCommand::RenameLocal { side, from, to });
    }

    pub fn remove_local(&self, side: SftpSide, path: PathBuf, recursive: bool) {
        self.worker.send(SftpCommand::RemoveLocal {
            side,
            path,
            recursive,
        });
    }

    pub fn transfer_file(
        &self,
        from_side: SftpSide,
        from_path: String,
        to_side: SftpSide,
        to_cwd: String,
        name: String,
    ) {
        self.worker.send(SftpCommand::Transfer {
            from_side,
            from_path,
            to_side,
            to_cwd,
            name,
        });
    }

    pub fn close(&self) {
        self.worker.send(SftpCommand::Close);
    }
}

pub fn wait_listed(worker: &SftpWorker, side: SftpSide) -> Vec<SftpListEntry> {
    for _ in 0..100 {
        for event in worker.drain() {
            if let SftpEvent::Listed {
                side: s,
                entries,
                ..
            } = event
            {
                if s == side {
                    return entries;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timeout waiting for Listed on {side:?}");
}

pub fn wait_closed(worker: &SftpWorker) {
    for _ in 0..100 {
        for event in worker.drain() {
            if matches!(event, SftpEvent::Closed) {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timeout waiting for Closed");
}

pub fn wait_event(
    worker: &SftpWorker,
    mut pred: impl FnMut(&SftpEvent) -> bool,
) -> SftpEvent {
    for _ in 0..100 {
        for event in worker.drain() {
            if pred(&event) {
                return event;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timeout waiting for matching SftpEvent");
}
