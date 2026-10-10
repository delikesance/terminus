use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use super::{row_ts, row_uuid, Store};
use crate::error::{Error, Result};
use crate::models::*;

impl Store {
    // --- Identity CRUD ---
    pub async fn upsert_identity(&self, identity: &Identity) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO identities (id, name, kind, public_key, private_key, passphrase, created_at, updated_at, deleted_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name=excluded.name, kind=excluded.kind, public_key=excluded.public_key,
                private_key=excluded.private_key, passphrase=excluded.passphrase, updated_at=excluded.updated_at, deleted_at=excluded.deleted_at
            "#,
        )
        .bind(identity.id.to_string())
        .bind(&identity.name)
        .bind(&identity.kind)
        .bind(&identity.public_key)
        .bind(&identity.private_key)
        .bind(&identity.passphrase)
        .bind(identity.created_at.to_rfc3339())
        .bind(identity.updated_at.to_rfc3339())
        .bind(identity.deleted_at.map(|d| d.to_rfc3339()))
        .execute(&self.pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
        Ok(())
    }

    pub async fn list_identities(&self) -> Result<Vec<Identity>> {
        let rows = sqlx::query("SELECT * FROM identities WHERE deleted_at IS NULL")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;

        Ok(rows
            .into_iter()
            .filter_map(|r| -> Option<Identity> {
                Some(Identity {
                    id: row_uuid(&r, "id")?,
                    name: r.try_get("name").ok()?,
                    kind: r.try_get("kind").ok()?,
                    public_key: r.try_get("public_key").ok()?,
                    private_key: r.try_get("private_key").ok()?,
                    passphrase: r.try_get("passphrase").ok()?,
                    created_at: row_ts(&r, "created_at")?,
                    updated_at: row_ts(&r, "updated_at")?,
                    deleted_at: r
                        .try_get::<Option<String>, _>("deleted_at")
                        .ok()
                        .flatten()
                        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                        .map(|d| d.with_timezone(&Utc)),
                })
            })
            .collect())
    }

    pub async fn get_identity(&self, id: Uuid) -> Result<Option<Identity>> {
        Ok(self
            .list_identities()
            .await?
            .into_iter()
            .find(|identity| identity.id == id))
    }

    pub async fn delete_identity(&self, id: Uuid) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE identities SET deleted_at = ?, updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(&now)
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;
        Ok(())
    }
}
