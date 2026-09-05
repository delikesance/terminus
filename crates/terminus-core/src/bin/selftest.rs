use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;
use terminus_core::models::*;
use terminus_core::pty;
use terminus_core::ssh;
use terminus_core::store::Store;
use terminus_core::sync::SyncEngine;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(report) => {
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
            if report["ok"].as_bool() == Some(true) {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(err) => {
            eprintln!("selftest aborted: {err:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<Value> {
    let mut checks = Vec::new();
    let tmp = tempfile_dir()?;
    let db_path = tmp.join("terminus.db");
    let store = Store::open(&db_path).await.context("open sqlite")?;

    checks.push(check("sqlite_open", store.path().exists(), "local sqlite created"));

    let appearance = TerminalAppearance {
        font_size: 16.0,
        theme_id: "phosphor".into(),
        renderer: "webgl".into(),
        ..TerminalAppearance::default()
    };
    store.set_appearance(&appearance).await?;
    let loaded = store.appearance().await?;
    checks.push(check(
        "appearance_roundtrip",
        loaded.font_size == 16.0 && loaded.theme_id == "phosphor",
        "terminal appearance persisted",
    ));

    let keys = store.keybindings().await?;
    checks.push(check(
        "keybindings_defaults",
        keys.contains_key("ctrl+k") && keys.contains_key("ctrl+shift+t"),
        "customizable keybindings present",
    ));

    let themes = ColorTheme::builtins();
    checks.push(check(
        "themes_builtin",
        themes.len() >= 4,
        "built-in color themes available",
    ));

    let pty_out = pty::run_local_command("echo", &["TERMINUS_PTY_OK"], Duration::from_secs(8))
        .context("local pty")?;
    let pty_text = String::from_utf8_lossy(&pty_out);
    checks.push(check(
        "local_pty",
        pty_text.contains("TERMINUS_PTY_OK"),
        format!("local pty output: {}", sanitize(&pty_text)),
    ));

    checks.push(interactive_session(&store).await);
    checks.push(pty_throughput());
    checks.push(session_throughput(&store).await);
    checks.push(rust_term_raster());

    let mut host = Host::new("selftest-ssh", "127.0.0.1", 2222, "terminus");
    host.password = Some("terminus".into());
    host.auth_method = "password".into();
    store.upsert_host(&host).await?;
    let listed = store.list_hosts().await?;
    checks.push(check(
        "host_inventory",
        listed.iter().any(|h| h.id == host.id),
        "SSH host stored locally",
    ));

    let mut snippet = Snippet::new("disk", "df -h");
    snippet.shortcut = Some("ctrl+alt+d".into());
    store.upsert_snippet(&snippet).await?;
    checks.push(check(
        "snippets",
        store.list_snippets().await?.iter().any(|s| s.id == snippet.id),
        "snippet stored",
    ));

    store
        .add_history(&HistoryEntry::new("echo hello", "local"))
        .await?;
    checks.push(check(
        "history",
        !store.search_history("echo", 10).await?.is_empty(),
        "command history searchable",
    ));

    let group = Group::new("lab");
    store.upsert_group(&group).await?;
    checks.push(check(
        "groups",
        store.list_groups().await?.iter().any(|g| g.id == group.id),
        "host groups stored",
    ));

    let ssh_host = std::env::var("TERMINUS_SSH_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let ssh_port: u16 = std::env::var("TERMINUS_SSH_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(2222);
    host.hostname = ssh_host;
    host.port = ssh_port;
    store.upsert_host(&host).await?;

    match ssh::exec_command(&host, None, "echo TERMINUS_SSH_OK && pwd").await {
        Ok(out) => checks.push(check(
            "ssh_exec",
            out.contains("TERMINUS_SSH_OK"),
            format!("ssh output: {}", sanitize(&out)),
        )),
        Err(err) => checks.push(check("ssh_exec", false, format!("ssh failed: {err}"))),
    }

    checks.push(ssh_key_auth(&host, &tmp).await);

    match ssh::sftp_roundtrip(&host, None, "/tmp/terminus-selftest.txt", b"terminus-sftp").await {
        Ok(bytes) => checks.push(check(
            "sftp_list",
            bytes == b"terminus-sftp",
            format!("sftp roundtrip {} bytes", bytes.len()),
        )),
        Err(err) => checks.push(check("sftp_list", false, format!("sftp failed: {err}"))),
    }

    let pg_url = std::env::var("TERMINUS_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://terminus:terminus@127.0.0.1:54329/terminus".into()
    });
    let engine = SyncEngine::new(store.clone());
    match engine
        .configure(SyncConfig {
            url: pg_url.clone(),
            sync_secrets: true,
        })
        .await
    {
        Ok(()) => {
            checks.push(check("postgres_connect", true, "connected to postgres"));
            match engine.sync_now().await {
                Ok(stats) => checks.push(check(
                    "sql_sync_push",
                    true,
                    format!("push/pull {}", stats),
                )),
                Err(err) => checks.push(check("sql_sync_push", false, err.to_string())),
            }

            let store_b = Store::open(tmp.join("replica.db")).await?;
            let engine_b = SyncEngine::new(store_b.clone());
            engine_b
                .configure(SyncConfig {
                    url: pg_url,
                    sync_secrets: true,
                })
                .await?;
            engine_b.sync_now().await?;
            let replica_hosts = store_b.list_hosts().await?;
            let replica_snips = store_b.list_snippets().await?;
            checks.push(check(
                "sql_sync_pull",
                replica_hosts.iter().any(|h| h.id == host.id)
                    && replica_snips.iter().any(|s| s.id == snippet.id),
                "second client pulled hosts and snippets from postgres",
            ));
        }
        Err(err) => {
            checks.push(check("postgres_connect", false, err.to_string()));
            checks.push(check("sql_sync_push", false, "skipped"));
            checks.push(check("sql_sync_pull", false, "skipped"));
        }
    }

    let required = [
        "sqlite_open",
        "appearance_roundtrip",
        "keybindings_defaults",
        "themes_builtin",
        "local_pty",
        "interactive_session",
        "pty_throughput",
        "session_throughput",
        "rust_term_raster",
        "host_inventory",
        "snippets",
        "history",
        "groups",
        "ssh_exec",
        "ssh_key",
        "sftp_list",
        "postgres_connect",
        "sql_sync_push",
        "sql_sync_pull",
    ];
    let ok = required.iter().all(|name| {
        checks
            .iter()
            .find(|c| c["name"] == *name)
            .and_then(|c| c["ok"].as_bool())
            == Some(true)
    });

    Ok(json!({
        "ok": ok,
        "checked_at": Utc::now().to_rfc3339(),
        "checks": checks,
    }))
}

struct CollectingSink {
    buf: tokio::sync::Mutex<Vec<u8>>,
}

#[async_trait::async_trait]
impl terminus_core::OutputSink for CollectingSink {
    async fn emit_output(&self, _session_id: &str, data: &[u8]) {
        self.buf.lock().await.extend_from_slice(data);
    }
    async fn emit_exit(&self, _session_id: &str) {}
}

async fn interactive_session(store: &Store) -> Value {
    let sink = std::sync::Arc::new(CollectingSink {
        buf: tokio::sync::Mutex::new(Vec::new()),
    });
    let manager = terminus_core::SessionManager::new(store.clone(), sink.clone());
    match manager.open_local(80, 24, 1.0).await {
        Ok(info) => {
            let _ = manager.write(&info.id, b"echo TERMINUS_SESSION_OK\r").await;
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            let mut ok = false;
            while std::time::Instant::now() < deadline {
                let text = String::from_utf8_lossy(&*sink.buf.lock().await).into_owned();
                if text.contains("TERMINUS_SESSION_OK") {
                    ok = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            let text = String::from_utf8_lossy(&*sink.buf.lock().await).into_owned();
            let _ = manager.close(&info.id);
            check(
                "interactive_session",
                ok,
                format!("session output: {}", sanitize(&text)),
            )
        }
        Err(err) => check("interactive_session", false, err.to_string()),
    }
}

fn rust_term_raster() -> Value {
    match terminus_core::term::TerminalEmulator::new(40, 12, 14.0) {
        Ok(mut term) => {
            term.feed(b"echo RUST_TERM_OK\r\n");
            let frame = term.raster();
            let painted = frame
                .rgba
                .chunks(4)
                .filter(|px| *px != [28u8, 28, 30, 255])
                .count();
            check(
                "rust_term_raster",
                frame.width > 0 && painted > 80,
                format!("{}x{} raster, {painted} painted pixels", frame.width, frame.height),
            )
        }
        Err(err) => check("rust_term_raster", false, err.to_string()),
    }
}

fn pty_throughput() -> Value {
    let started = std::time::Instant::now();
    match pty::run_local_command(
        "dd",
        &["if=/dev/zero", "bs=65536", "count=8", "status=none"],
        Duration::from_secs(5),
    ) {
        Ok(out) => {
            let elapsed = started.elapsed().as_secs_f64().max(0.000_1);
            let mbs = out.len() as f64 / elapsed / 1_000_000.0;
            check(
                "pty_throughput",
                out.len() >= 400_000 && elapsed < 2.5,
                format!("{} bytes in {elapsed:.3}s ({mbs:.1} MB/s)", out.len()),
            )
        }
        Err(err) => check("pty_throughput", false, err.to_string()),
    }
}

async fn session_throughput(store: &Store) -> Value {
    let sink = std::sync::Arc::new(CollectingSink {
        buf: tokio::sync::Mutex::new(Vec::new()),
    });
    let manager = terminus_core::SessionManager::new(store.clone(), sink.clone());
    match manager.open_local(80, 24, 1.0).await {
        Ok(info) => {
            let started = std::time::Instant::now();
            let _ = manager
                .write(&info.id, b"dd if=/dev/zero bs=32768 count=8 status=none\r")
                .await;
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            let mut got = 0usize;
            while std::time::Instant::now() < deadline {
                got = sink.buf.lock().await.len();
                if got >= 200_000 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            let elapsed = started.elapsed().as_secs_f64().max(0.000_1);
            let mbs = got as f64 / elapsed / 1_000_000.0;
            let _ = manager.close(&info.id);
            check(
                "session_throughput",
                got >= 200_000 && elapsed < 3.0,
                format!("{got} bytes in {elapsed:.3}s ({mbs:.1} MB/s)"),
            )
        }
        Err(err) => check("session_throughput", false, err.to_string()),
    }
}

async fn ssh_key_auth(password_host: &Host, tmp: &std::path::Path) -> Value {
    let key_path = tmp.join("id_ed25519");
    let generated = std::process::Command::new("ssh-keygen")
        .args([
            "-t",
            "ed25519",
            "-f",
            &key_path.to_string_lossy(),
            "-N",
            "",
            "-q",
        ])
        .status();
    if !matches!(generated, Ok(s) if s.success()) {
        return check("ssh_key", false, "ssh-keygen failed");
    }
    let pub_key = match std::fs::read_to_string(key_path.with_extension("pub")) {
        Ok(key) => key.trim().to_string(),
        Err(err) => return check("ssh_key", false, err.to_string()),
    };
    let install = format!(
        "mkdir -p ~/.ssh && chmod 700 ~/.ssh && grep -qxF '{pub_key}' ~/.ssh/authorized_keys 2>/dev/null || echo '{pub_key}' >> ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys"
    );
    if let Err(err) = ssh::exec_command(password_host, None, &install).await {
        return check("ssh_key", false, format!("install pubkey: {err}"));
    }
    let mut key_host = password_host.clone();
    key_host.auth_method = "key".into();
    key_host.password = None;
    let mut identity = Identity::new("selftest-key");
    identity.private_key = Some(key_path.to_string_lossy().into_owned());
    match ssh::exec_command(&key_host, Some(&identity), "echo TERMINUS_KEY_OK").await {
        Ok(out) => check(
            "ssh_key",
            out.contains("TERMINUS_KEY_OK"),
            format!("key auth output: {}", sanitize(&out)),
        ),
        Err(err) => check("ssh_key", false, err.to_string()),
    }
}

fn check(name: &str, ok: bool, detail: impl Into<String>) -> Value {
    json!({ "name": name, "ok": ok, "detail": detail.into() })
}

fn sanitize(input: &str) -> String {
    input
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == ' ')
        .take(160)
        .collect()
}

fn tempfile_dir() -> Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("terminus-selftest-{}", std::process::id()));
    std::fs::create_dir_all(&dir).with_context(|| format!("mkdir {}", dir.display()))?;
    Ok(dir)
}

#[allow(dead_code)]
fn require_ok(report: &Value) -> Result<()> {
    if report["ok"].as_bool() != Some(true) {
        bail!("selftest failed");
    }
    Ok(())
}
