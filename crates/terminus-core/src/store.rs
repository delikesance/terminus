use crate::error::Result;
use crate::models::*;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use sqlx::Row;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
    path: PathBuf,
}

impl Store {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let opts = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(opts)
            .await?;
        let store = Self { pool, path };
        store.migrate().await?;
        Ok(store)
    }

    pub async fn open_default() -> Result<Self> {
        let dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("terminus");
        Self::open(dir.join("terminus.db")).await
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    async fn migrate(&self) -> Result<()> {
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
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              deleted_at TEXT
            );
            CREATE TABLE IF NOT EXISTS groups (
              id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              parent_id TEXT,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              deleted_at TEXT
            );
            CREATE TABLE IF NOT EXISTS identities (
              id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              public_key TEXT,
              private_key TEXT,
              passphrase TEXT,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              deleted_at TEXT
            );
            CREATE TABLE IF NOT EXISTS snippets (
              id TEXT PRIMARY KEY,
              title TEXT NOT NULL,
              content TEXT NOT NULL,
              tags TEXT NOT NULL DEFAULT '[]',
              shortcut TEXT,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              deleted_at TEXT
            );
            CREATE TABLE IF NOT EXISTS history (
              id TEXT PRIMARY KEY,
              command TEXT NOT NULL,
              cwd TEXT,
              host_id TEXT,
              session_kind TEXT NOT NULL DEFAULT 'local',
              created_at TEXT NOT NULL
            );
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
            );
            CREATE TABLE IF NOT EXISTS settings (
              key TEXT PRIMARY KEY,
              value TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_hosts_updated ON hosts(updated_at);
            CREATE INDEX IF NOT EXISTS idx_snippets_updated ON snippets(updated_at);
            CREATE INDEX IF NOT EXISTS idx_history_created ON history(created_at);
            "#,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn upsert_host(&self, host: &Host) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO hosts (id,name,hostname,port,username,auth_method,password,identity_id,group_id,tags,notes,created_at,updated_at,deleted_at)
               VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)
               ON CONFLICT(id) DO UPDATE SET
                 name=excluded.name, hostname=excluded.hostname, port=excluded.port,
                 username=excluded.username, auth_method=excluded.auth_method, password=excluded.password,
                 identity_id=excluded.identity_id, group_id=excluded.group_id, tags=excluded.tags,
                 notes=excluded.notes, updated_at=excluded.updated_at, deleted_at=excluded.deleted_at"#,
        )
        .bind(&host.id)
        .bind(&host.name)
        .bind(&host.hostname)
        .bind(host.port as i64)
        .bind(&host.username)
        .bind(&host.auth_method)
        .bind(&host.password)
        .bind(&host.identity_id)
        .bind(&host.group_id)
        .bind(serde_json::to_string(&host.tags)?)
        .bind(&host.notes)
        .bind(host.created_at.to_rfc3339())
        .bind(host.updated_at.to_rfc3339())
        .bind(host.deleted_at.map(|d| d.to_rfc3339()))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_hosts(&self) -> Result<Vec<Host>> {
        let rows = sqlx::query("SELECT * FROM hosts WHERE deleted_at IS NULL ORDER BY name")
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(row_to_host).collect()
    }

    pub async fn get_host(&self, id: &str) -> Result<Option<Host>> {
        let row = sqlx::query("SELECT * FROM hosts WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(row_to_host).transpose()
    }

    pub async fn delete_host(&self, id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE hosts SET deleted_at = ?, updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(&now)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn upsert_group(&self, group: &Group) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO groups (id,name,parent_id,created_at,updated_at,deleted_at)
               VALUES (?,?,?,?,?,?)
               ON CONFLICT(id) DO UPDATE SET
                 name=excluded.name, parent_id=excluded.parent_id,
                 updated_at=excluded.updated_at, deleted_at=excluded.deleted_at"#,
        )
        .bind(&group.id)
        .bind(&group.name)
        .bind(&group.parent_id)
        .bind(group.created_at.to_rfc3339())
        .bind(group.updated_at.to_rfc3339())
        .bind(group.deleted_at.map(|d| d.to_rfc3339()))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_groups(&self) -> Result<Vec<Group>> {
        let rows = sqlx::query("SELECT * FROM groups WHERE deleted_at IS NULL ORDER BY name")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| Group {
                id: row.get("id"),
                name: row.get("name"),
                parent_id: row.get("parent_id"),
                created_at: parse_dt(row.get("created_at")),
                updated_at: parse_dt(row.get("updated_at")),
                deleted_at: row.get::<Option<String>, _>("deleted_at").map(parse_dt),
            })
            .collect())
    }

    pub async fn upsert_identity(&self, identity: &Identity) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO identities (id,name,public_key,private_key,passphrase,created_at,updated_at,deleted_at)
               VALUES (?,?,?,?,?,?,?,?)
               ON CONFLICT(id) DO UPDATE SET
                 name=excluded.name, public_key=excluded.public_key, private_key=excluded.private_key,
                 passphrase=excluded.passphrase, updated_at=excluded.updated_at, deleted_at=excluded.deleted_at"#,
        )
        .bind(&identity.id)
        .bind(&identity.name)
        .bind(&identity.public_key)
        .bind(&identity.private_key)
        .bind(&identity.passphrase)
        .bind(identity.created_at.to_rfc3339())
        .bind(identity.updated_at.to_rfc3339())
        .bind(identity.deleted_at.map(|d| d.to_rfc3339()))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_identities(&self) -> Result<Vec<Identity>> {
        let rows = sqlx::query("SELECT * FROM identities WHERE deleted_at IS NULL ORDER BY name")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| Identity {
                id: row.get("id"),
                name: row.get("name"),
                public_key: row.get("public_key"),
                private_key: row.get("private_key"),
                passphrase: row.get("passphrase"),
                created_at: parse_dt(row.get("created_at")),
                updated_at: parse_dt(row.get("updated_at")),
                deleted_at: row.get::<Option<String>, _>("deleted_at").map(parse_dt),
            })
            .collect())
    }

    pub async fn get_identity(&self, id: &str) -> Result<Option<Identity>> {
        Ok(self
            .list_identities()
            .await?
            .into_iter()
            .find(|i| i.id == id))
    }

    pub async fn upsert_snippet(&self, snippet: &Snippet) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO snippets (id,title,content,tags,shortcut,created_at,updated_at,deleted_at)
               VALUES (?,?,?,?,?,?,?,?)
               ON CONFLICT(id) DO UPDATE SET
                 title=excluded.title, content=excluded.content, tags=excluded.tags,
                 shortcut=excluded.shortcut, updated_at=excluded.updated_at, deleted_at=excluded.deleted_at"#,
        )
        .bind(&snippet.id)
        .bind(&snippet.title)
        .bind(&snippet.content)
        .bind(serde_json::to_string(&snippet.tags)?)
        .bind(&snippet.shortcut)
        .bind(snippet.created_at.to_rfc3339())
        .bind(snippet.updated_at.to_rfc3339())
        .bind(snippet.deleted_at.map(|d| d.to_rfc3339()))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_snippets(&self) -> Result<Vec<Snippet>> {
        let rows = sqlx::query("SELECT * FROM snippets WHERE deleted_at IS NULL ORDER BY title")
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(row_to_snippet).collect()
    }

    pub async fn delete_snippet(&self, id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE snippets SET deleted_at = ?, updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(&now)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn add_history(&self, entry: &HistoryEntry) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO history (id,command,cwd,host_id,session_kind,created_at)
               VALUES (?,?,?,?,?,?)"#,
        )
        .bind(&entry.id)
        .bind(&entry.command)
        .bind(&entry.cwd)
        .bind(&entry.host_id)
        .bind(&entry.session_kind)
        .bind(entry.created_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn search_history(&self, query: &str, limit: i64) -> Result<Vec<HistoryEntry>> {
        let like = format!("%{query}%");
        let rows = sqlx::query(
            "SELECT * FROM history WHERE command LIKE ? ORDER BY created_at DESC LIMIT ?",
        )
        .bind(like)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| HistoryEntry {
                id: row.get("id"),
                command: row.get("command"),
                cwd: row.get("cwd"),
                host_id: row.get("host_id"),
                session_kind: row.get("session_kind"),
                created_at: parse_dt(row.get("created_at")),
            })
            .collect())
    }

    pub async fn upsert_forward(&self, fwd: &PortForward) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO port_forwards (id,host_id,kind,name,bind_host,bind_port,dest_host,dest_port,created_at,updated_at,deleted_at)
               VALUES (?,?,?,?,?,?,?,?,?,?,?)
               ON CONFLICT(id) DO UPDATE SET
                 host_id=excluded.host_id, kind=excluded.kind, name=excluded.name,
                 bind_host=excluded.bind_host, bind_port=excluded.bind_port,
                 dest_host=excluded.dest_host, dest_port=excluded.dest_port,
                 updated_at=excluded.updated_at, deleted_at=excluded.deleted_at"#,
        )
        .bind(&fwd.id)
        .bind(&fwd.host_id)
        .bind(&fwd.kind)
        .bind(&fwd.name)
        .bind(&fwd.bind_host)
        .bind(fwd.bind_port as i64)
        .bind(&fwd.dest_host)
        .bind(fwd.dest_port.map(|p| p as i64))
        .bind(fwd.created_at.to_rfc3339())
        .bind(fwd.updated_at.to_rfc3339())
        .bind(fwd.deleted_at.map(|d| d.to_rfc3339()))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_forwards(&self) -> Result<Vec<PortForward>> {
        let rows =
            sqlx::query("SELECT * FROM port_forwards WHERE deleted_at IS NULL ORDER BY name")
                .fetch_all(&self.pool)
                .await?;
        Ok(rows
            .into_iter()
            .map(|row| PortForward {
                id: row.get("id"),
                host_id: row.get("host_id"),
                kind: row.get("kind"),
                name: row.get("name"),
                bind_host: row.get("bind_host"),
                bind_port: row.get::<i64, _>("bind_port") as u16,
                dest_host: row.get("dest_host"),
                dest_port: row
                    .get::<Option<i64>, _>("dest_port")
                    .map(|p| p as u16),
                created_at: parse_dt(row.get("created_at")),
                updated_at: parse_dt(row.get("updated_at")),
                deleted_at: row.get::<Option<String>, _>("deleted_at").map(parse_dt),
            })
            .collect())
    }

    pub async fn set_setting(&self, key: &str, value: &Value) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"INSERT INTO settings (key,value,updated_at) VALUES (?,?,?)
               ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at"#,
        )
        .bind(key)
        .bind(serde_json::to_string(value)?)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_setting(&self, key: &str) -> Result<Option<Value>> {
        let row = sqlx::query("SELECT value FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        match row {
            Some(row) => {
                let raw: String = row.get("value");
                Ok(Some(serde_json::from_str(&raw)?))
            }
            None => Ok(None),
        }
    }

    pub async fn all_settings(&self) -> Result<serde_json::Map<String, Value>> {
        let rows = sqlx::query("SELECT key, value FROM settings")
            .fetch_all(&self.pool)
            .await?;
        let mut map = serde_json::Map::new();
        for row in rows {
            let key: String = row.get("key");
            let raw: String = row.get("value");
            map.insert(key, serde_json::from_str(&raw)?);
        }
        Ok(map)
    }

    pub async fn appearance(&self) -> Result<TerminalAppearance> {
        match self.get_setting("appearance").await? {
            Some(value) => Ok(serde_json::from_value(value)?),
            None => Ok(TerminalAppearance::default()),
        }
    }

    pub async fn set_appearance(&self, appearance: &TerminalAppearance) -> Result<()> {
        self.set_setting("appearance", &serde_json::to_value(appearance)?)
            .await
    }

    pub async fn keybindings(&self) -> Result<serde_json::Map<String, Value>> {
        match self.get_setting("keybindings").await? {
            Some(Value::Object(map)) => Ok(map),
            _ => Ok(default_keybindings()),
        }
    }

    pub async fn dump_table_json(&self, table: &str) -> Result<Vec<Value>> {
        let allowed = [
            "hosts",
            "groups",
            "identities",
            "snippets",
            "history",
            "port_forwards",
            "settings",
        ];
        if !allowed.contains(&table) {
            return Err(crate::error::Error::msg("unknown table"));
        }
        let rows = sqlx::query(&format!("SELECT * FROM {table}"))
            .fetch_all(&self.pool)
            .await?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row_to_json(&row)?);
        }
        Ok(out)
    }
}

fn default_keybindings() -> serde_json::Map<String, Value> {
    let pairs = [
        ("ctrl+shift+t", "tab.new"),
        ("ctrl+shift+w", "tab.close"),
        ("ctrl+tab", "tab.next"),
        ("ctrl+shift+tab", "tab.prev"),
        ("ctrl+shift+d", "tab.new"),
        ("ctrl+shift+e", "tab.new"),
        ("ctrl+k", "palette.toggle"),
        ("ctrl+,", "settings.toggle"),
        ("ctrl+shift+f", "search.toggle"),
        ("ctrl+shift+p", "command.palette"),
        ("ctrl+l", "terminal.clear"),
        ("ctrl+shift+c", "terminal.copy"),
        ("ctrl+shift+v", "terminal.paste"),
        ("ctrl+plus", "font.increase"),
        ("ctrl+minus", "font.decrease"),
        ("ctrl+0", "font.reset"),
        ("ctrl+b", "sidebar.toggle"),
        ("cmd+b", "sidebar.toggle"),
    ];
    pairs
        .into_iter()
        .map(|(k, v)| (k.to_string(), Value::String(v.into())))
        .collect()
}

fn parse_dt(raw: String) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&raw)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

fn row_to_host(row: sqlx::sqlite::SqliteRow) -> Result<Host> {
    let tags: String = row.get("tags");
    Ok(Host {
        id: row.get("id"),
        name: row.get("name"),
        hostname: row.get("hostname"),
        port: row.get::<i64, _>("port") as u16,
        username: row.get("username"),
        auth_method: row.get("auth_method"),
        password: row.get("password"),
        identity_id: row.get("identity_id"),
        group_id: row.get("group_id"),
        tags: serde_json::from_str(&tags).unwrap_or_default(),
        notes: row.get("notes"),
        created_at: parse_dt(row.get("created_at")),
        updated_at: parse_dt(row.get("updated_at")),
        deleted_at: row.get::<Option<String>, _>("deleted_at").map(parse_dt),
    })
}

fn row_to_snippet(row: sqlx::sqlite::SqliteRow) -> Result<Snippet> {
    let tags: String = row.get("tags");
    Ok(Snippet {
        id: row.get("id"),
        title: row.get("title"),
        content: row.get("content"),
        tags: serde_json::from_str(&tags).unwrap_or_default(),
        shortcut: row.get("shortcut"),
        created_at: parse_dt(row.get("created_at")),
        updated_at: parse_dt(row.get("updated_at")),
        deleted_at: row.get::<Option<String>, _>("deleted_at").map(parse_dt),
    })
}

fn row_to_json(row: &sqlx::sqlite::SqliteRow) -> Result<Value> {
    let mut map = serde_json::Map::new();
    for column in row.columns() {
        let name = column.name().to_string();
        let value: Value = match row.try_get_raw(column.ordinal()) {
            Ok(raw) if raw.is_null() => Value::Null,
            Ok(_) => {
                if let Ok(v) = row.try_get::<i64, _>(column.ordinal()) {
                    Value::from(v)
                } else if let Ok(v) = row.try_get::<f64, _>(column.ordinal()) {
                    serde_json::Number::from_f64(v)
                        .map(Value::Number)
                        .unwrap_or(Value::Null)
                } else if let Ok(v) = row.try_get::<String, _>(column.ordinal()) {
                    Value::String(v)
                } else {
                    Value::Null
                }
            }
            Err(_) => Value::Null,
        };
        map.insert(name, value);
    }
    Ok(Value::Object(map))
}

use sqlx::{Column, ValueRef};
