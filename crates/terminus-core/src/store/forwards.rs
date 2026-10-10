use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use super::{row_ts, row_uuid, Store};
use crate::error::{Error, Result};
use crate::models::*;

impl Store {
    // --- Port Forward CRUD ---
    pub async fn upsert_forward(&self, pf: &PortForward) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO port_forwards (id, host_id, kind, name, bind_host, bind_port, dest_host, dest_port, created_at, updated_at, deleted_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                host_id=excluded.host_id, kind=excluded.kind, name=excluded.name,
                bind_host=excluded.bind_host, bind_port=excluded.bind_port,
                dest_host=excluded.dest_host, dest_port=excluded.dest_port,
                updated_at=excluded.updated_at, deleted_at=excluded.deleted_at
            "#,
        )
        .bind(pf.id.to_string())
        .bind(pf.host_id.to_string())
        .bind(&pf.kind)
        .bind(&pf.name)
        .bind(&pf.bind_host)
        .bind(pf.bind_port as i64)
        .bind(&pf.dest_host)
        .bind(pf.dest_port.map(|p| p as i64))
        .bind(pf.created_at.to_rfc3339())
        .bind(pf.updated_at.to_rfc3339())
        .bind(pf.deleted_at.map(|d| d.to_rfc3339()))
        .execute(&self.pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
        Ok(())
    }

    /// Tombstone a forward (soft delete; the bumped `updated_at` lets sync
    /// order the deletion after every live copy).
    pub async fn delete_forward(&self, id: Uuid) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE port_forwards SET deleted_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&now)
        .bind(&now)
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
        Ok(())
    }

    pub async fn list_forwards(&self, host_id: Option<Uuid>) -> Result<Vec<PortForward>> {
        let query = if let Some(hid) = host_id {
            format!(
                "SELECT * FROM port_forwards WHERE deleted_at IS NULL AND host_id = '{}'",
                hid
            )
        } else {
            "SELECT * FROM port_forwards WHERE deleted_at IS NULL".into()
        };

        let rows = sqlx::query(&query)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;

        Ok(rows
            .into_iter()
            .filter_map(|r| -> Option<PortForward> {
                Some(PortForward {
                    id: row_uuid(&r, "id")?,
                    host_id: row_uuid(&r, "host_id")?,
                    kind: r.try_get("kind").ok()?,
                    name: r.try_get("name").ok()?,
                    bind_host: r.try_get("bind_host").ok()?,
                    bind_port: r.try_get::<i64, _>("bind_port").ok()? as u16,
                    dest_host: r.try_get("dest_host").ok()?,
                    dest_port: r
                        .try_get::<Option<i64>, _>("dest_port")
                        .ok()?
                        .map(|p| p as u16),
                    created_at: row_ts(&r, "created_at")?,
                    updated_at: row_ts(&r, "updated_at")?,
                    deleted_at: r
                        .try_get::<Option<String>, _>("deleted_at")
                        .ok()?
                        .and_then(|s| {
                            DateTime::parse_from_rfc3339(&s)
                                .ok()
                                .map(|d| d.with_timezone(&Utc))
                        }),
                })
            })
            .collect())
    }
}
