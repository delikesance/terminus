use crate::error::{Error, Result};
use crate::models::{SyncConfig, SyncStatus};
use crate::store::Store;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::Row;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct SyncEngine {
    store: Store,
    remote: Mutex<Option<PgPool>>,
    config: Mutex<Option<SyncConfig>>,
    last_sync: Mutex<Option<DateTime<Utc>>>,
    last_error: Mutex<Option<String>>,
    state: Mutex<String>, // "unconfigured" | "idle" | "syncing" | "offline" | "error"
}

impl SyncEngine {
    pub fn new(store: Store) -> Self {
        Self {
            store,
            remote: Mutex::new(None),
            config: Mutex::new(None),
            last_sync: Mutex::new(None),
            last_error: Mutex::new(None),
            state: Mutex::new("unconfigured".to_string()),
        }
    }

    pub async fn restore(&self) -> Result<()> {
        if let Some(value) = self.store.get_setting("sync").await? {
            if let Ok(cfg) = serde_json::from_value::<SyncConfig>(value) {
                if !cfg.url.is_empty() {
                    match self.configure(cfg).await {
                        Ok(()) => {
                            // Successfully restored, set to idle
                            *self.state.lock().await = "idle".to_string();
                        }
                        Err(_) => {
                            // Configuration failed, likely offline or error
                            *self.state.lock().await = "offline".to_string();
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn configure(&self, config: SyncConfig) -> Result<()> {
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .connect(&config.url)
            .await
            .map_err(|e| Error::msg(format!("postgres connect failed: {e}")))?;
        ensure_remote_schema(&pool).await?;
        self.store
            .set_setting("sync", &serde_json::to_value(&config)?)
            .await?;
        *self.remote.lock().await = Some(pool);
        *self.config.lock().await = Some(config);
        *self.last_error.lock().await = None;
        // Successfully configured, transition to idle
        *self.state.lock().await = "idle".to_string();
        Ok(())
    }

    pub async fn status(&self) -> SyncStatus {
        let configured = self.remote.lock().await.is_some();
        let state = if !configured {
            "unconfigured".to_string()
        } else {
            self.state.lock().await.clone()
        };
        
        SyncStatus {
            configured,
            url: self.config.lock().await.as_ref().map(|c| c.url.clone()),
            last_sync: *self.last_sync.lock().await,
            last_error: self.last_error.lock().await.clone(),
            state,
        }
    }

    pub async fn sync_now(&self) -> Result<Value> {
        let pool = self
            .remote
            .lock()
            .await
            .clone()
            .ok_or(Error::SyncNotConfigured)?;
        let sync_secrets = self
            .config
            .lock()
            .await
            .as_ref()
            .map(|c| c.sync_secrets)
            .unwrap_or(false);

        // Transition to syncing state
        *self.state.lock().await = "syncing".to_string();

        let result = async {
            let pushed = self.push_all(&pool, sync_secrets).await?;
            let pulled = self.pull_all(&pool, sync_secrets).await?;
            Ok::<_, Error>(json!({ "pushed": pushed, "pulled": pulled }))
        }
        .await;

        match result {
            Ok(stats) => {
                *self.last_sync.lock().await = Some(Utc::now());
                *self.last_error.lock().await = None;
                // Success: transition to idle and clear last_error
                *self.state.lock().await = "idle".to_string();
                Ok(stats)
            }
            Err(err) => {
                let err_str = err.to_string();
                *self.last_error.lock().await = Some(err_str.clone());
                
                // Determine if it's offline (network/connectivity) or error (business logic)
                // Network errors typically contain keywords like "connection", "timeout", "unreachable"
                let is_network_error = err_str.to_lowercase().contains("connection")
                    || err_str.to_lowercase().contains("timeout")
                    || err_str.to_lowercase().contains("unreachable")
                    || err_str.to_lowercase().contains("network");
                
                if is_network_error {
                    *self.state.lock().await = "offline".to_string();
                } else {
                    *self.state.lock().await = "error".to_string();
                }
                
                Err(err)
            }
        }
    }

    async fn push_all(&self, pool: &PgPool, sync_secrets: bool) -> Result<usize> {
        let mut count = 0;
        count += push_rows(pool, "hosts", &filter_hosts(self.store.dump_table_json("hosts").await?, sync_secrets)).await?;
        count += push_rows(pool, "groups", &self.store.dump_table_json("groups").await?).await?;
        count += push_rows(
            pool,
            "identities",
            &filter_identities(self.store.dump_table_json("identities").await?, sync_secrets),
        )
        .await?;
        count += push_rows(pool, "snippets", &self.store.dump_table_json("snippets").await?).await?;
        count += push_rows(pool, "history", &self.store.dump_table_json("history").await?).await?;
        count += push_rows(
            pool,
            "port_forwards",
            &self.store.dump_table_json("port_forwards").await?,
        )
        .await?;
        Ok(count)
    }

    async fn pull_all(&self, pool: &PgPool, sync_secrets: bool) -> Result<usize> {
        let mut count = 0;
        count += self.pull_hosts(pool, sync_secrets).await?;
        count += self.pull_groups(pool).await?;
        count += self.pull_identities(pool, sync_secrets).await?;
        count += self.pull_snippets(pool).await?;
        count += self.pull_history(pool).await?;
        count += self.pull_forwards(pool).await?;
        Ok(count)
    }

    async fn pull_hosts(&self, pool: &PgPool, sync_secrets: bool) -> Result<usize> {
        let rows = sqlx::query("SELECT * FROM hosts").fetch_all(pool).await?;
        let mut n = 0;
        for row in rows {
            let mut host = crate::models::Host {
                id: row.get("id"),
                name: row.get("name"),
                hostname: row.get("hostname"),
                port: row.get::<i32, _>("port") as u16,
                username: row.get("username"),
                auth_method: row.get("auth_method"),
                password: row.get("password"),
                identity_id: row.get("identity_id"),
                group_id: row.get("group_id"),
                tags: serde_json::from_str(row.get::<String, _>("tags").as_str()).unwrap_or_default(),
                notes: row.get("notes"),
                created_at: parse_pg(row.get("created_at")),
                updated_at: parse_pg(row.get("updated_at")),
                deleted_at: row
                    .get::<Option<String>, _>("deleted_at")
                    .map(parse_pg),
            };
            if !sync_secrets {
                if let Some(existing) = self.store.get_host(&host.id).await? {
                    host.password = existing.password;
                } else {
                    host.password = None;
                }
            }
            if let Some(existing) = self.store.get_host(&host.id).await? {
                if existing.updated_at > host.updated_at {
                    continue;
                }
            }
            self.store.upsert_host(&host).await?;
            n += 1;
        }
        Ok(n)
    }

    async fn pull_groups(&self, pool: &PgPool) -> Result<usize> {
        let rows = sqlx::query("SELECT * FROM groups").fetch_all(pool).await?;
        let mut n = 0;
        for row in rows {
            let group = crate::models::Group {
                id: row.get("id"),
                name: row.get("name"),
                parent_id: row.get("parent_id"),
                created_at: parse_pg(row.get("created_at")),
                updated_at: parse_pg(row.get("updated_at")),
                deleted_at: row
                    .get::<Option<String>, _>("deleted_at")
                    .map(parse_pg),
            };
            self.store.upsert_group(&group).await?;
            n += 1;
        }
        Ok(n)
    }

    async fn pull_identities(&self, pool: &PgPool, sync_secrets: bool) -> Result<usize> {
        let rows = sqlx::query("SELECT * FROM identities").fetch_all(pool).await?;
        let mut n = 0;
        for row in rows {
            let mut identity = crate::models::Identity {
                id: row.get("id"),
                name: row.get("name"),
                public_key: row.get("public_key"),
                private_key: row.get("private_key"),
                passphrase: row.get("passphrase"),
                created_at: parse_pg(row.get("created_at")),
                updated_at: parse_pg(row.get("updated_at")),
                deleted_at: row
                    .get::<Option<String>, _>("deleted_at")
                    .map(parse_pg),
            };
            if !sync_secrets {
                identity.private_key = None;
                identity.passphrase = None;
            }
            self.store.upsert_identity(&identity).await?;
            n += 1;
        }
        Ok(n)
    }

    async fn pull_snippets(&self, pool: &PgPool) -> Result<usize> {
        let rows = sqlx::query("SELECT * FROM snippets").fetch_all(pool).await?;
        let mut n = 0;
        for row in rows {
            let snippet = crate::models::Snippet {
                id: row.get("id"),
                title: row.get("title"),
                content: row.get("content"),
                tags: serde_json::from_str(row.get::<String, _>("tags").as_str()).unwrap_or_default(),
                shortcut: row.get("shortcut"),
                created_at: parse_pg(row.get("created_at")),
                updated_at: parse_pg(row.get("updated_at")),
                deleted_at: row
                    .get::<Option<String>, _>("deleted_at")
                    .map(parse_pg),
            };
            self.store.upsert_snippet(&snippet).await?;
            n += 1;
        }
        Ok(n)
    }

    async fn pull_history(&self, pool: &PgPool) -> Result<usize> {
        let rows = sqlx::query("SELECT * FROM history").fetch_all(pool).await?;
        let mut n = 0;
        for row in rows {
            let entry = crate::models::HistoryEntry {
                id: row.get("id"),
                command: row.get("command"),
                cwd: row.get("cwd"),
                host_id: row.get("host_id"),
                session_kind: row.get("session_kind"),
                created_at: parse_pg(row.get("created_at")),
            };
            // ignore unique violations by checking first
            let existing = self.store.search_history(&entry.command, 50).await?;
            if existing.iter().any(|e| e.id == entry.id) {
                continue;
            }
            self.store.add_history(&entry).await?;
            n += 1;
        }
        Ok(n)
    }

    async fn pull_forwards(&self, pool: &PgPool) -> Result<usize> {
        let rows = sqlx::query("SELECT * FROM port_forwards")
            .fetch_all(pool)
            .await?;
        let mut n = 0;
        for row in rows {
            let fwd = crate::models::PortForward {
                id: row.get("id"),
                host_id: row.get("host_id"),
                kind: row.get("kind"),
                name: row.get("name"),
                bind_host: row.get("bind_host"),
                bind_port: row.get::<i32, _>("bind_port") as u16,
                dest_host: row.get("dest_host"),
                dest_port: row.get::<Option<i32>, _>("dest_port").map(|p| p as u16),
                created_at: parse_pg(row.get("created_at")),
                updated_at: parse_pg(row.get("updated_at")),
                deleted_at: row
                    .get::<Option<String>, _>("deleted_at")
                    .map(parse_pg),
            };
            self.store.upsert_forward(&fwd).await?;
            n += 1;
        }
        Ok(n)
    }
}

pub fn shared(store: Store) -> Arc<SyncEngine> {
    Arc::new(SyncEngine::new(store))
}

fn parse_pg(raw: String) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&raw)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

fn filter_hosts(mut rows: Vec<Value>, sync_secrets: bool) -> Vec<Value> {
    if sync_secrets {
        return rows;
    }
    for row in &mut rows {
        if let Some(obj) = row.as_object_mut() {
            obj.insert("password".into(), Value::Null);
        }
    }
    rows
}

fn filter_identities(mut rows: Vec<Value>, sync_secrets: bool) -> Vec<Value> {
    if sync_secrets {
        return rows;
    }
    for row in &mut rows {
        if let Some(obj) = row.as_object_mut() {
            obj.insert("private_key".into(), Value::Null);
            obj.insert("passphrase".into(), Value::Null);
        }
    }
    rows
}

async fn ensure_remote_schema(pool: &PgPool) -> Result<()> {
    let statements = [
        r#"CREATE TABLE IF NOT EXISTS hosts (
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
        )"#,
        r#"CREATE TABLE IF NOT EXISTS groups (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          parent_id TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          deleted_at TEXT
        )"#,
        r#"CREATE TABLE IF NOT EXISTS identities (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          public_key TEXT,
          private_key TEXT,
          passphrase TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          deleted_at TEXT
        )"#,
        r#"CREATE TABLE IF NOT EXISTS snippets (
          id TEXT PRIMARY KEY,
          title TEXT NOT NULL,
          content TEXT NOT NULL,
          tags TEXT NOT NULL DEFAULT '[]',
          shortcut TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          deleted_at TEXT
        )"#,
        r#"CREATE TABLE IF NOT EXISTS history (
          id TEXT PRIMARY KEY,
          command TEXT NOT NULL,
          cwd TEXT,
          host_id TEXT,
          session_kind TEXT NOT NULL DEFAULT 'local',
          created_at TEXT NOT NULL
        )"#,
        r#"CREATE TABLE IF NOT EXISTS port_forwards (
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
        )"#,
    ];
    for sql in statements {
        sqlx::query(sql).execute(pool).await?;
    }
    Ok(())
}

async fn push_rows(pool: &PgPool, table: &str, rows: &[Value]) -> Result<usize> {
    let mut n = 0;
    for row in rows {
        let obj = row.as_object().ok_or_else(|| Error::msg("invalid row"))?;
        match table {
            "hosts" => {
                sqlx::query(
                    r#"INSERT INTO hosts (id,name,hostname,port,username,auth_method,password,identity_id,group_id,tags,notes,created_at,updated_at,deleted_at)
                       VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
                       ON CONFLICT (id) DO UPDATE SET
                         name=EXCLUDED.name, hostname=EXCLUDED.hostname, port=EXCLUDED.port,
                         username=EXCLUDED.username, auth_method=EXCLUDED.auth_method, password=EXCLUDED.password,
                         identity_id=EXCLUDED.identity_id, group_id=EXCLUDED.group_id, tags=EXCLUDED.tags,
                         notes=EXCLUDED.notes, updated_at=EXCLUDED.updated_at, deleted_at=EXCLUDED.deleted_at
                         WHERE hosts.updated_at <= EXCLUDED.updated_at"#,
                )
                .bind(str_field(obj, "id"))
                .bind(str_field(obj, "name"))
                .bind(str_field(obj, "hostname"))
                .bind(int_field(obj, "port"))
                .bind(str_field(obj, "username"))
                .bind(str_field(obj, "auth_method"))
                .bind(opt_str(obj, "password"))
                .bind(opt_str(obj, "identity_id"))
                .bind(opt_str(obj, "group_id"))
                .bind(str_field(obj, "tags"))
                .bind(str_field(obj, "notes"))
                .bind(str_field(obj, "created_at"))
                .bind(str_field(obj, "updated_at"))
                .bind(opt_str(obj, "deleted_at"))
                .execute(pool)
                .await?;
            }
            "groups" => {
                sqlx::query(
                    r#"INSERT INTO groups (id,name,parent_id,created_at,updated_at,deleted_at)
                       VALUES ($1,$2,$3,$4,$5,$6)
                       ON CONFLICT (id) DO UPDATE SET
                         name=EXCLUDED.name, parent_id=EXCLUDED.parent_id,
                         updated_at=EXCLUDED.updated_at, deleted_at=EXCLUDED.deleted_at"#,
                )
                .bind(str_field(obj, "id"))
                .bind(str_field(obj, "name"))
                .bind(opt_str(obj, "parent_id"))
                .bind(str_field(obj, "created_at"))
                .bind(str_field(obj, "updated_at"))
                .bind(opt_str(obj, "deleted_at"))
                .execute(pool)
                .await?;
            }
            "identities" => {
                sqlx::query(
                    r#"INSERT INTO identities (id,name,public_key,private_key,passphrase,created_at,updated_at,deleted_at)
                       VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
                       ON CONFLICT (id) DO UPDATE SET
                         name=EXCLUDED.name, public_key=EXCLUDED.public_key, private_key=EXCLUDED.private_key,
                         passphrase=EXCLUDED.passphrase, updated_at=EXCLUDED.updated_at, deleted_at=EXCLUDED.deleted_at"#,
                )
                .bind(str_field(obj, "id"))
                .bind(str_field(obj, "name"))
                .bind(opt_str(obj, "public_key"))
                .bind(opt_str(obj, "private_key"))
                .bind(opt_str(obj, "passphrase"))
                .bind(str_field(obj, "created_at"))
                .bind(str_field(obj, "updated_at"))
                .bind(opt_str(obj, "deleted_at"))
                .execute(pool)
                .await?;
            }
            "snippets" => {
                sqlx::query(
                    r#"INSERT INTO snippets (id,title,content,tags,shortcut,created_at,updated_at,deleted_at)
                       VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
                       ON CONFLICT (id) DO UPDATE SET
                         title=EXCLUDED.title, content=EXCLUDED.content, tags=EXCLUDED.tags,
                         shortcut=EXCLUDED.shortcut, updated_at=EXCLUDED.updated_at, deleted_at=EXCLUDED.deleted_at"#,
                )
                .bind(str_field(obj, "id"))
                .bind(str_field(obj, "title"))
                .bind(str_field(obj, "content"))
                .bind(str_field(obj, "tags"))
                .bind(opt_str(obj, "shortcut"))
                .bind(str_field(obj, "created_at"))
                .bind(str_field(obj, "updated_at"))
                .bind(opt_str(obj, "deleted_at"))
                .execute(pool)
                .await?;
            }
            "history" => {
                sqlx::query(
                    r#"INSERT INTO history (id,command,cwd,host_id,session_kind,created_at)
                       VALUES ($1,$2,$3,$4,$5,$6)
                       ON CONFLICT (id) DO NOTHING"#,
                )
                .bind(str_field(obj, "id"))
                .bind(str_field(obj, "command"))
                .bind(opt_str(obj, "cwd"))
                .bind(opt_str(obj, "host_id"))
                .bind(str_field(obj, "session_kind"))
                .bind(str_field(obj, "created_at"))
                .execute(pool)
                .await?;
            }
            "port_forwards" => {
                sqlx::query(
                    r#"INSERT INTO port_forwards (id,host_id,kind,name,bind_host,bind_port,dest_host,dest_port,created_at,updated_at,deleted_at)
                       VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
                       ON CONFLICT (id) DO UPDATE SET
                         host_id=EXCLUDED.host_id, kind=EXCLUDED.kind, name=EXCLUDED.name,
                         bind_host=EXCLUDED.bind_host, bind_port=EXCLUDED.bind_port,
                         dest_host=EXCLUDED.dest_host, dest_port=EXCLUDED.dest_port,
                         updated_at=EXCLUDED.updated_at, deleted_at=EXCLUDED.deleted_at"#,
                )
                .bind(str_field(obj, "id"))
                .bind(str_field(obj, "host_id"))
                .bind(str_field(obj, "kind"))
                .bind(str_field(obj, "name"))
                .bind(str_field(obj, "bind_host"))
                .bind(int_field(obj, "bind_port"))
                .bind(opt_str(obj, "dest_host"))
                .bind(opt_int(obj, "dest_port"))
                .bind(str_field(obj, "created_at"))
                .bind(str_field(obj, "updated_at"))
                .bind(opt_str(obj, "deleted_at"))
                .execute(pool)
                .await?;
            }
            _ => {}
        }
        n += 1;
    }
    Ok(n)
}

fn str_field(obj: &serde_json::Map<String, Value>, key: &str) -> String {
    obj.get(key)
        .and_then(|v| match v {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        })
        .unwrap_or_default()
}

fn opt_str(obj: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    match obj.get(key) {
        Some(Value::Null) | None => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
    }
}

fn int_field(obj: &serde_json::Map<String, Value>, key: &str) -> i32 {
    match obj.get(key) {
        Some(Value::Number(n)) => n.as_i64().unwrap_or(0) as i32,
        Some(Value::String(s)) => s.parse().unwrap_or(0),
        _ => 0,
    }
}

fn opt_int(obj: &serde_json::Map<String, Value>, key: &str) -> Option<i32> {
    match obj.get(key) {
        Some(Value::Null) | None => None,
        Some(Value::Number(n)) => n.as_i64().map(|v| v as i32),
        Some(Value::String(s)) => s.parse().ok(),
        _ => None,
    }
}
