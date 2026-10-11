use sqlx::{Row, SqlitePool};

use super::Store;
use crate::error::{Error, Result};

impl Store {
    pub(super) async fn migrate(pool: &SqlitePool) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS hosts (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                hostname TEXT NOT NULL,
                port INTEGER NOT NULL DEFAULT 22,
                username TEXT NOT NULL,
                auth_method TEXT NOT NULL DEFAULT 'password',
                password TEXT,
                identity_id TEXT,
                group_id TEXT,
                tags TEXT NOT NULL DEFAULT '[]',
                notes TEXT NOT NULL DEFAULT '',
                os_id TEXT,
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                deleted_at TEXT
            )
            "#,
        )
        .execute(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS groups (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                parent_id TEXT,
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                deleted_at TEXT
            )
            "#,
        )
        .execute(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS identities (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                kind TEXT NOT NULL DEFAULT 'key',
                public_key TEXT,
                private_key TEXT,
                passphrase TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                deleted_at TEXT
            )
            "#,
        )
        .execute(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS snippets (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                tags TEXT NOT NULL DEFAULT '[]',
                shortcut TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                deleted_at TEXT
            )
            "#,
        )
        .execute(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS history (
                id TEXT PRIMARY KEY,
                command TEXT NOT NULL,
                cwd TEXT,
                host_id TEXT,
                session_kind TEXT NOT NULL,
                created_at TEXT NOT NULL
            )
            "#,
        )
        .execute(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS port_forwards (
                id TEXT PRIMARY KEY,
                host_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                name TEXT NOT NULL,
                bind_host TEXT NOT NULL DEFAULT '127.0.0.1',
                bind_port INTEGER NOT NULL,
                dest_host TEXT,
                dest_port INTEGER,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                deleted_at TEXT
            )
            "#,
        )
        .execute(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT ''
            )
            "#,
        )
        .execute(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

        // Older installs created `settings` without `updated_at`. CREATE IF NOT
        // EXISTS is a no-op then, so add the column when missing. Live Windows
        // DBs already have `updated_at TEXT NOT NULL` from a prior schema —
        // `set_setting` must write it (see below).
        Self::ensure_settings_updated_at(pool).await?;
        Self::ensure_column(pool, "snippets", "host_id", "TEXT").await?;
        Self::ensure_sort_order(pool, "hosts").await?;
        Self::ensure_sort_order(pool, "groups").await?;
        // Same story for the columns `upsert_host` writes: a `hosts` table made
        // by an older build keeps its old shape, and saving a host then fails
        // with "table hosts has no column named os_id".
        for (column, ddl) in [
            ("password", "TEXT"),
            ("identity_id", "TEXT"),
            ("group_id", "TEXT"),
            ("tags", "TEXT NOT NULL DEFAULT '[]'"),
            ("notes", "TEXT NOT NULL DEFAULT ''"),
            ("os_id", "TEXT"),
            ("deleted_at", "TEXT"),
        ] {
            Self::ensure_column(pool, "hosts", column, ddl).await?;
        }

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS credentials (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                owner_kind TEXT NOT NULL,
                owner_id TEXT NOT NULL,
                envelope TEXT NOT NULL,
                key_id TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                deleted_at TEXT
            )
            "#,
        )
        .execute(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;

        Ok(())
    }

    async fn ensure_settings_updated_at(pool: &SqlitePool) -> Result<()> {
        let rows = sqlx::query("PRAGMA table_info(settings)")
            .fetch_all(pool)
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;
        let has_updated_at = rows.iter().any(|r| {
            r.try_get::<String, _>("name")
                .map(|n| n == "updated_at")
                .unwrap_or(false)
        });
        if !has_updated_at {
            sqlx::query(
                "ALTER TABLE settings ADD COLUMN updated_at TEXT NOT NULL DEFAULT ''",
            )
            .execute(pool)
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;
        }
        Ok(())
    }

    /// Add `column` to `table` when an older schema lacks it.
    async fn ensure_column(
        pool: &SqlitePool,
        table: &str,
        column: &str,
        ddl: &str,
    ) -> Result<()> {
        let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
            .fetch_all(pool)
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;
        let has = rows.iter().any(|r| {
            r.try_get::<String, _>("name")
                .map(|n| n == column)
                .unwrap_or(false)
        });
        if !has {
            sqlx::query(&format!("ALTER TABLE {table} ADD COLUMN {column} {ddl}"))
                .execute(pool)
                .await
                .map_err(|e| Error::DatabaseError(e.to_string()))?;
        }
        Ok(())
    }

    async fn ensure_sort_order(pool: &SqlitePool, table: &str) -> Result<()> {
        let pragma = format!("PRAGMA table_info({table})");
        let rows = sqlx::query(&pragma)
            .fetch_all(pool)
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;
        let has = rows.iter().any(|r| {
            r.try_get::<String, _>("name")
                .map(|n| n == "sort_order")
                .unwrap_or(false)
        });
        if !has {
            let alter = format!(
                "ALTER TABLE {table} ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0"
            );
            sqlx::query(&alter)
                .execute(pool)
                .await
                .map_err(|e| Error::DatabaseError(e.to_string()))?;
        }
        Ok(())
    }
}
