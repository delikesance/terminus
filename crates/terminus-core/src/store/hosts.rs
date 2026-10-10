use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use super::{row_ts, row_uuid, Store};
use crate::error::{Error, Result};
use crate::models::*;

impl Store {
    // --- Host CRUD ---
    pub async fn upsert_host(&self, host: &Host) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO hosts (id, name, hostname, port, username, auth_method, password, identity_id, group_id, tags, notes, os_id, sort_order, created_at, updated_at, deleted_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name=excluded.name, hostname=excluded.hostname, port=excluded.port,
                username=excluded.username, auth_method=excluded.auth_method, password=excluded.password,
                identity_id=excluded.identity_id, group_id=excluded.group_id, tags=excluded.tags,
                notes=excluded.notes, os_id=excluded.os_id, sort_order=excluded.sort_order,
                updated_at=excluded.updated_at, deleted_at=excluded.deleted_at
            "#,
        )
        .bind(host.id.to_string())
        .bind(&host.name)
        .bind(&host.hostname)
        .bind(host.port as i64)
        .bind(&host.username)
        .bind(&host.auth_method)
        .bind(&host.password)
        .bind(host.identity_id.map(|u| u.to_string()))
        .bind(host.group_id.map(|u| u.to_string()))
        .bind(serde_json::to_string(&host.tags).unwrap())
        .bind(&host.notes)
        .bind(&host.os_id)
        .bind(host.sort_order)
        .bind(host.created_at.to_rfc3339())
        .bind(host.updated_at.to_rfc3339())
        .bind(host.deleted_at.map(|d| d.to_rfc3339()))
        .execute(&self.pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
        Ok(())
    }

    pub async fn list_hosts(&self) -> Result<Vec<Host>> {
        let rows = sqlx::query("SELECT * FROM hosts WHERE deleted_at IS NULL")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;

        Ok(rows
            .into_iter()
            .filter_map(|r| -> Option<Host> {
                Some(Host {
                    id: row_uuid(&r, "id")?,
                    name: r.try_get("name").ok()?,
                    hostname: r.try_get("hostname").ok()?,
                    port: r.try_get::<i64, _>("port").ok()? as u16,
                    username: r.try_get("username").ok()?,
                    auth_method: r.try_get("auth_method").ok()?,
                    password: r.try_get("password").ok()?,
                    identity_id: r
                        .try_get::<Option<String>, _>("identity_id")
                        .ok()
                        .flatten()
                        .and_then(|s| Uuid::parse_str(&s).ok()),
                    group_id: r
                        .try_get::<Option<String>, _>("group_id")
                        .ok()
                        .flatten()
                        .and_then(|s| Uuid::parse_str(&s).ok()),
                    tags: serde_json::from_str(&r.try_get::<String, _>("tags").ok()?)
                        .unwrap_or_default(),
                    notes: r.try_get("notes").ok()?,
                    os_id: r.try_get("os_id").ok()?,
                    sort_order: r.try_get::<i64, _>("sort_order").unwrap_or(0),
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

    /// Soft-delete a host and the secrets it owns.
    ///
    /// The tombstone bumps `updated_at` so last-writer-wins sync orders it
    /// after every live copy of the row on other devices.
    pub async fn delete_host(&self, id: Uuid) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;
        sqlx::query("UPDATE hosts SET deleted_at = ?, updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(&now)
            .bind(id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;
        sqlx::query(
            "UPDATE credentials SET deleted_at = ?, updated_at = ? \
             WHERE owner_kind = 'host' AND owner_id = ? AND deleted_at IS NULL",
        )
        .bind(&now)
        .bind(&now)
        .bind(id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
        sqlx::query(
            "UPDATE port_forwards SET deleted_at = ?, updated_at = ? \
             WHERE host_id = ? AND deleted_at IS NULL",
        )
        .bind(&now)
        .bind(&now)
        .bind(id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
        tx.commit()
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;
        Ok(())
    }

    /// Soft-delete a group and clear membership on hosts that pointed at it.
    pub async fn delete_group(&self, id: Uuid) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;
        sqlx::query("UPDATE hosts SET group_id = NULL, updated_at = ? WHERE group_id = ? AND deleted_at IS NULL")
            .bind(&now)
            .bind(id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;
        sqlx::query("UPDATE groups SET deleted_at = ?, updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(&now)
            .bind(id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;
        tx.commit()
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;
        Ok(())
    }
}
