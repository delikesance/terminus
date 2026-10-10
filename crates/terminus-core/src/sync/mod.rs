//! Sync engine: keeps the local store convergent with a remote backend.
//!
//! The remote backend is a sqlx [`SqlitePool`] today (a PostgreSQL pool can be
//! dropped in behind the same handle once the transport lands). The engine
//! itself owns the *state machine* and the bookkeeping around a sync run:
//!
//! ```text
//!                    ┌──────────────┐
//!  configured  ┌────▶│ Unconfigured │
//!              │     └──────┬───────┘
//!              │            │ remote attached
//!              │            ▼
//!  ┌───────────┴──┐   ┌─────────┐   network lost   ┌─────────┐
//!  │     Idle     │◀─▶│ Syncing │─────────────────▶│ Offline │
//!  └──────┬───────┘   └────┬────┘                  └────┬────┘
//!         │                │ transport/build failure   │
//!         │                ▼                           │
//!         │           ┌─────────┐                      │
//!         └──────────▶│  Error  │◀─────────────────────┘
//!                     └─────────┘
//! ```
//!
//! `push_all()` / `pull_all()` merge the local store with the remote table by
//! table, last-writer-wins on `updated_at`. Soft-delete tombstones are rows
//! too, so deletions propagate. Vault envelopes (`credentials`, sealed
//! `identities`) only move with `sync_secrets` and a matching vault header.

mod config;
mod engine;
mod report;
mod status;
mod tables;
#[cfg(test)]
mod tests;

pub use config::{open_remote_sqlite_pool, SyncConfig};
pub use engine::SyncEngine;
pub use report::SyncReport;
pub use status::SyncStatus;
use tables::*;
