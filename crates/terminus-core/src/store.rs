//! Local persistence layer (SQLx/SQLite).
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    SqlitePool,
};
use std::path::PathBuf;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::models::*;
use chrono::{DateTime, Utc};
use sqlx::Row;

#[derive(Debug, Clone)]
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    pub async fn open(data_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&data_dir)?;
        let db_path = data_dir.join("terminus.db");

        // Options are built directly instead of using a
        // `sqlite://<path>?foreign_keys=on` URL: sqlx 0.7's SQLite URL parser
        // only accepts the `mode` and `cache` query parameters and rejects
        // every other one, so the URL form failed to open at all. This also
        // creates the database file when it does not exist yet.
        let options = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(options)
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;

        Self::migrate(&pool).await?;
        let store = Self { pool };
        store.ensure_default_root_order().await?;
        Ok(store)
    }

    async fn migrate(pool: &SqlitePool) -> Result<()> {
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
        Self::ensure_sort_order(pool, "hosts").await?;
        Self::ensure_sort_order(pool, "groups").await?;

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
            .map(|r| Host {
                id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap(),
                name: r.get("name"),
                hostname: r.get("hostname"),
                port: r.get::<i64, _>("port") as u16,
                username: r.get("username"),
                auth_method: r.get("auth_method"),
                password: r.get("password"),
                identity_id: r
                    .get::<Option<String>, _>("identity_id")
                    .and_then(|s| Uuid::parse_str(&s).ok()),
                group_id: r
                    .get::<Option<String>, _>("group_id")
                    .and_then(|s| Uuid::parse_str(&s).ok()),
                tags: serde_json::from_str(&r.get::<String, _>("tags"))
                    .unwrap_or_default(),
                notes: r.get("notes"),
                os_id: r.get("os_id"),
                sort_order: r.try_get::<i64, _>("sort_order").unwrap_or(0),
                created_at: DateTime::parse_from_rfc3339(
                    &r.get::<String, _>("created_at"),
                )
                .unwrap()
                .with_timezone(&Utc),
                updated_at: DateTime::parse_from_rfc3339(
                    &r.get::<String, _>("updated_at"),
                )
                .unwrap()
                .with_timezone(&Utc),
                deleted_at: r.get::<Option<String>, _>("deleted_at").and_then(|s| {
                    DateTime::parse_from_rfc3339(&s)
                        .ok()
                        .map(|d| d.with_timezone(&Utc))
                }),
            })
            .collect())
    }

    pub async fn delete_host(&self, id: Uuid) -> Result<()> {
        sqlx::query("UPDATE hosts SET deleted_at = ? WHERE id = ?")
            .bind(Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
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

    // --- Group CRUD ---
    pub async fn upsert_group(&self, group: &Group) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO groups (id, name, parent_id, sort_order, created_at, updated_at, deleted_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name=excluded.name, parent_id=excluded.parent_id, sort_order=excluded.sort_order,
                updated_at=excluded.updated_at, deleted_at=excluded.deleted_at
            "#,
        )
        .bind(group.id.to_string())
        .bind(&group.name)
        .bind(group.parent_id.map(|u| u.to_string()))
        .bind(group.sort_order)
        .bind(group.created_at.to_rfc3339())
        .bind(group.updated_at.to_rfc3339())
        .bind(group.deleted_at.map(|d| d.to_rfc3339()))
        .execute(&self.pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
        Ok(())
    }

    pub async fn list_groups(&self) -> Result<Vec<Group>> {
        let rows = sqlx::query("SELECT * FROM groups WHERE deleted_at IS NULL")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|r| Group {
                id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap(),
                name: r.get("name"),
                parent_id: r
                    .get::<Option<String>, _>("parent_id")
                    .and_then(|s| Uuid::parse_str(&s).ok()),
                sort_order: r.try_get::<i64, _>("sort_order").unwrap_or(0),
                created_at: DateTime::parse_from_rfc3339(
                    &r.get::<String, _>("created_at"),
                )
                .unwrap()
                .with_timezone(&Utc),
                updated_at: DateTime::parse_from_rfc3339(
                    &r.get::<String, _>("updated_at"),
                )
                .unwrap()
                .with_timezone(&Utc),
                deleted_at: r.get::<Option<String>, _>("deleted_at").and_then(|s| {
                    DateTime::parse_from_rfc3339(&s)
                        .ok()
                        .map(|d| d.with_timezone(&Utc))
                }),
            })
            .collect())
    }

    /// Next `sort_order` for a host joining `group_id`.
    /// For ungrouped hosts, shares the root sequence with groups.
    pub async fn next_host_sort_order(&self, group_id: Option<Uuid>) -> Result<i64> {
        match group_id {
            Some(gid) => {
                let row = sqlx::query(
                    "SELECT COALESCE(MAX(sort_order), -1) AS m FROM hosts WHERE deleted_at IS NULL AND group_id = ?",
                )
                .bind(gid.to_string())
                .fetch_one(&self.pool)
                .await
                .map_err(|e| Error::DatabaseError(e.to_string()))?;
                Ok(row.get::<i64, _>("m") + 1)
            }
            None => self.next_root_sort_order().await,
        }
    }

    pub async fn next_group_sort_order(&self) -> Result<i64> {
        self.next_root_sort_order().await
    }

    /// Next sort_order in the shared root list (ungrouped hosts + groups).
    pub async fn next_root_sort_order(&self) -> Result<i64> {
        let host_max = sqlx::query(
            "SELECT COALESCE(MAX(sort_order), -1) AS m FROM hosts WHERE deleted_at IS NULL AND group_id IS NULL",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?
        .get::<i64, _>("m");
        let group_max = sqlx::query(
            "SELECT COALESCE(MAX(sort_order), -1) AS m FROM groups WHERE deleted_at IS NULL",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?
        .get::<i64, _>("m");
        Ok(host_max.max(group_max) + 1)
    }

    /// Anchor for inserting into the shared root list.
    pub async fn reorder_root(
        &self,
        moving_is_group: bool,
        moving_id: Uuid,
        before_is_group: Option<bool>,
        before_id: Option<Uuid>,
    ) -> Result<()> {
        #[derive(Clone)]
        enum RootEntry {
            Host(Host),
            Group(Group),
        }

        let hosts = self.list_hosts().await?;
        let groups = self.list_groups().await?;

        let mut root: Vec<RootEntry> = Vec::new();
        for h in hosts
            .iter()
            .filter(|h| h.group_id.is_none() && h.deleted_at.is_none())
        {
            if moving_is_group || h.id != moving_id {
                root.push(RootEntry::Host(h.clone()));
            }
        }
        for g in groups.iter().filter(|g| g.deleted_at.is_none()) {
            if !moving_is_group || g.id != moving_id {
                root.push(RootEntry::Group(g.clone()));
            }
        }
        root.sort_by(|a, b| {
            let (oa, na) = match a {
                RootEntry::Host(h) => (h.sort_order, h.name.to_lowercase()),
                RootEntry::Group(g) => (g.sort_order, g.name.to_lowercase()),
            };
            let (ob, nb) = match b {
                RootEntry::Host(h) => (h.sort_order, h.name.to_lowercase()),
                RootEntry::Group(g) => (g.sort_order, g.name.to_lowercase()),
            };
            oa.cmp(&ob).then_with(|| na.cmp(&nb))
        });

        let insert_at = match (before_is_group, before_id) {
            (Some(true), Some(id)) => root
                .iter()
                .position(|e| matches!(e, RootEntry::Group(g) if g.id == id))
                .unwrap_or(root.len()),
            (Some(false), Some(id)) => root
                .iter()
                .position(|e| matches!(e, RootEntry::Host(h) if h.id == id))
                .unwrap_or(root.len()),
            _ => root.len(),
        };

        let moved = if moving_is_group {
            let mut g = groups
                .into_iter()
                .find(|g| g.id == moving_id)
                .ok_or_else(|| Error::DatabaseError("group not found".into()))?;
            g.updated_at = Utc::now();
            RootEntry::Group(g)
        } else {
            let mut h = hosts
                .into_iter()
                .find(|h| h.id == moving_id)
                .ok_or_else(|| Error::DatabaseError("host not found".into()))?;
            h.group_id = None;
            h.updated_at = Utc::now();
            RootEntry::Host(h)
        };
        root.insert(insert_at, moved);

        for (i, entry) in root.into_iter().enumerate() {
            match entry {
                RootEntry::Host(mut h) => {
                    h.sort_order = i as i64;
                    self.upsert_host(&h).await?;
                }
                RootEntry::Group(mut g) => {
                    g.sort_order = i as i64;
                    self.upsert_group(&g).await?;
                }
            }
        }
        Ok(())
    }

    /// One-shot: hosts first (A–Z), then groups (A–Z). Skipped once applied.
    pub async fn ensure_default_root_order(&self) -> Result<()> {
        const KEY: &str = "sidebar_root_order_v1";
        if self.get_setting(KEY).await?.is_some() {
            return Ok(());
        }
        let mut hosts: Vec<Host> = self
            .list_hosts()
            .await?
            .into_iter()
            .filter(|h| h.group_id.is_none() && h.deleted_at.is_none())
            .collect();
        hosts.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        let mut groups: Vec<Group> = self
            .list_groups()
            .await?
            .into_iter()
            .filter(|g| g.deleted_at.is_none())
            .collect();
        groups.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        let mut i: i64 = 0;
        for mut h in hosts {
            h.sort_order = i;
            i += 1;
            self.upsert_host(&h).await?;
        }
        for mut g in groups {
            g.sort_order = i;
            i += 1;
            self.upsert_group(&g).await?;
        }
        self.set_setting(KEY, "1").await?;
        Ok(())
    }

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
            .map(|r| Identity {
                id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap(),
                name: r.get("name"),
                kind: r.get("kind"),
                public_key: r.get("public_key"),
                private_key: r.get("private_key"),
                passphrase: r.get("passphrase"),
                created_at: DateTime::parse_from_rfc3339(
                    &r.get::<String, _>("created_at"),
                )
                .unwrap()
                .with_timezone(&Utc),
                updated_at: DateTime::parse_from_rfc3339(
                    &r.get::<String, _>("updated_at"),
                )
                .unwrap()
                .with_timezone(&Utc),
                deleted_at: r
                    .get::<Option<String>, _>("deleted_at")
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|d| d.with_timezone(&Utc)),
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
        sqlx::query("UPDATE identities SET deleted_at = ? WHERE id = ?")
            .bind(Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;
        Ok(())
    }

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
            .map(|r| Snippet {
                id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap(),
                title: r.get("title"),
                content: r.get("content"),
                tags: serde_json::from_str(&r.get::<String, _>("tags"))
                    .unwrap_or_default(),
                shortcut: r.get("shortcut"),
                created_at: DateTime::parse_from_rfc3339(
                    &r.get::<String, _>("created_at"),
                )
                .unwrap()
                .with_timezone(&Utc),
                updated_at: DateTime::parse_from_rfc3339(
                    &r.get::<String, _>("updated_at"),
                )
                .unwrap()
                .with_timezone(&Utc),
                deleted_at: r.get::<Option<String>, _>("deleted_at").and_then(|s| {
                    DateTime::parse_from_rfc3339(&s)
                        .ok()
                        .map(|d| d.with_timezone(&Utc))
                }),
            })
            .collect())
    }

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

        Ok(rows
            .into_iter()
            .map(|r| HistoryEntry {
                id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap(),
                command: r.get("command"),
                cwd: r.get("cwd"),
                host_id: r
                    .get::<Option<String>, _>("host_id")
                    .and_then(|s| Uuid::parse_str(&s).ok()),
                session_kind: r.get("session_kind"),
                created_at: DateTime::parse_from_rfc3339(
                    &r.get::<String, _>("created_at"),
                )
                .unwrap()
                .with_timezone(&Utc),
            })
            .collect())
    }

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
            .map(|r| PortForward {
                id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap(),
                host_id: Uuid::parse_str(&r.get::<String, _>("host_id")).unwrap(),
                kind: r.get("kind"),
                name: r.get("name"),
                bind_host: r.get("bind_host"),
                bind_port: r.get::<i64, _>("bind_port") as u16,
                dest_host: r.get("dest_host"),
                dest_port: r.get::<Option<i64>, _>("dest_port").map(|p| p as u16),
                created_at: DateTime::parse_from_rfc3339(
                    &r.get::<String, _>("created_at"),
                )
                .unwrap()
                .with_timezone(&Utc),
                updated_at: DateTime::parse_from_rfc3339(
                    &r.get::<String, _>("updated_at"),
                )
                .unwrap()
                .with_timezone(&Utc),
                deleted_at: r.get::<Option<String>, _>("deleted_at").and_then(|s| {
                    DateTime::parse_from_rfc3339(&s)
                        .ok()
                        .map(|d| d.with_timezone(&Utc))
                }),
            })
            .collect())
    }

    // --- Settings ---
    pub async fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT value FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;
        Ok(row.map(|r| r.get("value")))
    }

    pub async fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO settings (key, value, updated_at) VALUES (?, ?, ?)
            ON CONFLICT(key) DO UPDATE SET
                value=excluded.value,
                updated_at=excluded.updated_at
            "#,
        )
        .bind(key)
        .bind(value)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
        Ok(())
    }

    // --- Credentials ---
    pub async fn upsert_credential(&self, cred: &Credential) -> Result<()> {
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
        .execute(&self.pool)
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
