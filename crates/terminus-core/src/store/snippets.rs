use chrono::{DateTime, Utc};
use sqlx::Row;

use super::{row_ts, row_uuid, Store};
use crate::error::{Error, Result};
use crate::models::*;

impl Store {
    // --- Snippet CRUD ---
    pub async fn upsert_snippet(&self, snippet: &Snippet) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO snippets (id, title, content, tags, shortcut, created_at, updated_at, deleted_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                title=excluded.title, content=excluded.content, tags=excluded.tags,
                shortcut=excluded.shortcut, updated_at=excluded.updated_at, deleted_at=excluded.deleted_at
            "#,
        )
        .bind(snippet.id.to_string())
        .bind(&snippet.title)
        .bind(&snippet.content)
        .bind(serde_json::to_string(&snippet.tags).unwrap())
        .bind(&snippet.shortcut)
        .bind(snippet.created_at.to_rfc3339())
        .bind(snippet.updated_at.to_rfc3339())
        .bind(snippet.deleted_at.map(|d| d.to_rfc3339()))
        .execute(&self.pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
        Ok(())
    }

    pub async fn list_snippets(&self) -> Result<Vec<Snippet>> {
        let rows = sqlx::query("SELECT * FROM snippets WHERE deleted_at IS NULL")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;

        Ok(rows
            .into_iter()
            .filter_map(|r| -> Option<Snippet> {
                Some(Snippet {
                    id: row_uuid(&r, "id")?,
                    title: r.try_get("title").ok()?,
                    content: r.try_get("content").ok()?,
                    tags: serde_json::from_str(&r.try_get::<String, _>("tags").ok()?)
                        .unwrap_or_default(),
                    shortcut: r.try_get("shortcut").ok()?,
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
