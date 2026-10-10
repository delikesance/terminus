use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use super::Store;
use crate::error::{Error, Result};
use crate::models::*;

impl Store {
    // --- Credentials ---
    pub async fn upsert_credential(&self, cred: &Credential) -> Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;
        sqlx::query(
            r#"
            INSERT INTO credentials (id, kind, owner_kind, owner_id, envelope, key_id, created_at, updated_at, deleted_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                kind=excluded.kind, owner_kind=excluded.owner_kind, owner_id=excluded.owner_id,
                envelope=excluded.envelope, key_id=excluded.key_id, updated_at=excluded.updated_at, deleted_at=excluded.deleted_at
            "#,
        )
        .bind(cred.id.to_string())
        .bind(&cred.kind)
        .bind(&cred.owner_kind)
        .bind(cred.owner_id.to_string())
        .bind(&cred.envelope)
        .bind(&cred.key_id)
        .bind(cred.created_at.to_rfc3339())
        .bind(cred.updated_at.to_rfc3339())
        .bind(cred.deleted_at.map(|d| d.to_rfc3339()))
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
        // One live secret per (owner, kind): retire rows written under an
        // older id scheme so lookups by owner cannot pick a stale envelope.
        if cred.deleted_at.is_none() {
            sqlx::query(
                "UPDATE credentials SET deleted_at = ?, updated_at = ? \
                 WHERE owner_kind = ? AND owner_id = ? AND kind = ? AND id != ? AND deleted_at IS NULL",
            )
            .bind(cred.updated_at.to_rfc3339())
            .bind(cred.updated_at.to_rfc3339())
            .bind(&cred.owner_kind)
            .bind(cred.owner_id.to_string())
            .bind(&cred.kind)
            .bind(cred.id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;
        }
        tx.commit()
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;
        Ok(())
    }

    /// Fetch one credential by id.
    pub async fn get_credential(&self, id: Uuid) -> Result<Option<Credential>> {
        let row = sqlx::query(
            "SELECT id, kind, owner_kind, owner_id, envelope, key_id, created_at, updated_at, deleted_at FROM credentials WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
        Ok(row.map(|r| Credential {
            id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_default(),
            kind: r.get("kind"),
            owner_kind: r.get("owner_kind"),
            owner_id: Uuid::parse_str(&r.get::<String, _>("owner_id"))
                .unwrap_or_default(),
            envelope: r.get("envelope"),
            key_id: r.get("key_id"),
            created_at: DateTime::parse_from_rfc3339(&r.get::<String, _>("created_at"))
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: DateTime::parse_from_rfc3339(&r.get::<String, _>("updated_at"))
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            deleted_at: r.get::<Option<String>, _>("deleted_at").and_then(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|d| d.with_timezone(&Utc))
            }),
        }))
    }

    /// Credentials owned by `(owner_kind, owner_id)`.
    pub async fn list_credentials_for_owner(
        &self,
        owner_kind: &str,
        owner_id: Uuid,
    ) -> Result<Vec<Credential>> {
        let rows = sqlx::query(
            "SELECT id, kind, owner_kind, owner_id, envelope, key_id, created_at, updated_at, deleted_at FROM credentials WHERE owner_kind = ? AND owner_id = ? AND deleted_at IS NULL",
        )
        .bind(owner_kind)
        .bind(owner_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|r| Credential {
                id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap_or_default(),
                kind: r.get("kind"),
                owner_kind: r.get("owner_kind"),
                owner_id: Uuid::parse_str(&r.get::<String, _>("owner_id"))
                    .unwrap_or_default(),
                envelope: r.get("envelope"),
                key_id: r.get("key_id"),
                created_at: DateTime::parse_from_rfc3339(
                    &r.get::<String, _>("created_at"),
                )
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
                updated_at: DateTime::parse_from_rfc3339(
                    &r.get::<String, _>("updated_at"),
                )
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
                deleted_at: r.get::<Option<String>, _>("deleted_at").and_then(|s| {
                    DateTime::parse_from_rfc3339(&s)
                        .ok()
                        .map(|d| d.with_timezone(&Utc))
                }),
            })
            .collect())
    }
}
