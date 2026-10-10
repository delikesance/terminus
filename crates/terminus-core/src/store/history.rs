use sqlx::Row;
use uuid::Uuid;

use super::{row_ts, row_uuid, Store};
use crate::error::{Error, Result};
use crate::models::*;

impl Store {
    // --- History CRUD ---
    pub async fn insert_history(&self, entry: &HistoryEntry) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO history (id, command, cwd, host_id, session_kind, created_at)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(entry.id.to_string())
        .bind(&entry.command)
        .bind(&entry.cwd)
        .bind(entry.host_id.map(|u| u.to_string()))
        .bind(&entry.session_kind)
        .bind(entry.created_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
        Ok(())
    }

    pub async fn list_history(&self, limit: usize) -> Result<Vec<HistoryEntry>> {
        let rows = sqlx::query(&format!(
            "SELECT * FROM history ORDER BY created_at DESC LIMIT {}",
            limit
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

        Ok(rows.into_iter().filter_map(history_from_row).collect())
    }

    /// Newest-first history of one machine (a sidebar row id: `local`,
    /// `wsl:<distro>` or a host uuid), see [`crate::history::machine_columns`].
    pub async fn list_history_for_machine(
        &self,
        row_id: &str,
        limit: usize,
    ) -> Result<Vec<HistoryEntry>> {
        let (host_id, kind) = crate::history::machine_columns(row_id);
        let query = match host_id {
            Some(id) => sqlx::query(
                "SELECT * FROM history WHERE host_id = ? ORDER BY created_at DESC LIMIT ?",
            )
            .bind(id.to_string()),
            None => sqlx::query(
                "SELECT * FROM history WHERE host_id IS NULL AND session_kind = ? ORDER BY created_at DESC LIMIT ?",
            )
            .bind(kind),
        };
        let rows = query
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;
        Ok(rows.into_iter().filter_map(history_from_row).collect())
    }
}

fn history_from_row(r: sqlx::sqlite::SqliteRow) -> Option<HistoryEntry> {
    Some(HistoryEntry {
        id: row_uuid(&r, "id")?,
        command: r.try_get("command").ok()?,
        cwd: r.try_get("cwd").ok()?,
        host_id: r
            .try_get::<Option<String>, _>("host_id")
            .ok()
            .flatten()
            .and_then(|s| Uuid::parse_str(&s).ok()),
        session_kind: r.try_get("session_kind").ok()?,
        created_at: row_ts(&r, "created_at")?,
    })
}
