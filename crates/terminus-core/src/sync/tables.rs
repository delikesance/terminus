use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy)]
pub(super) enum Direction {
    Push,
    Pull,
}

/// Tables synced for everyone. Soft-delete tombstones travel as rows.
pub(super) const PLAIN_TABLES: &[&str] =
    &["groups", "hosts", "snippets", "port_forwards"];
/// Tables holding vault envelopes; synced only with `sync_secrets` and a
/// matching vault header on both sides.
pub(super) const SECRET_TABLES: &[&str] = &["identities", "credentials"];
/// Columns never copied to a remote (legacy plaintext secrets).
pub(super) const NEVER_PUSHED: &[(&str, &str)] = &[("hosts", "password")];

pub(super) async fn ensure_meta_table(pool: &SqlitePool) -> Result<()> {
    sqlx::query("CREATE TABLE IF NOT EXISTS sync_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
        .execute(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
    Ok(())
}

/// Secrets are only exchanged between devices sharing one vault (same
/// salt + wrapped key); the first device to sync secrets publishes its header.
pub(super) async fn check_vault_match(
    local: &SqlitePool,
    remote: &SqlitePool,
) -> std::result::Result<(), String> {
    use sqlx::Row;
    let db = |e: sqlx::Error| e.to_string();
    let local_header: Option<String> =
        sqlx::query("SELECT value FROM settings WHERE key = ?")
            .bind(crate::vault::VAULT_HEADER_SETTING)
            .fetch_optional(local)
            .await
            .map_err(db)?
            .map(|r| r.get("value"));
    let Some(local_header) = local_header else {
        return Err("secrets not synced: this device has no vault".into());
    };
    let remote_header: Option<String> =
        sqlx::query("SELECT value FROM sync_meta WHERE key = ?")
            .bind(crate::vault::VAULT_HEADER_SETTING)
            .fetch_optional(remote)
            .await
            .map_err(db)?
            .map(|r| r.get("value"));
    match remote_header {
        None => {
            sqlx::query("INSERT INTO sync_meta (key, value) VALUES (?, ?)")
                .bind(crate::vault::VAULT_HEADER_SETTING)
                .bind(&local_header)
                .execute(remote)
                .await
                .map_err(db)?;
            Ok(())
        }
        Some(remote_header) => {
            let same = match (
                crate::vault::parse_vault_header(&local_header),
                crate::vault::parse_vault_header(&remote_header),
            ) {
                (Ok(a), Ok(b)) => a.salt == b.salt && a.wrapped_dek == b.wrapped_dek,
                _ => false,
            };
            if same {
                Ok(())
            } else {
                Err("secrets not synced: this device's vault differs from the synced vault".into())
            }
        }
    }
}

/// One SQLite value, copied verbatim between databases.
#[derive(Debug, Clone)]
pub(super) enum SqlValue {
    Null,
    Int(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

pub(super) fn read_value(row: &sqlx::sqlite::SqliteRow, idx: usize) -> SqlValue {
    use sqlx::{Row, TypeInfo, ValueRef};
    let Ok(raw) = row.try_get_raw(idx) else {
        return SqlValue::Null;
    };
    if raw.is_null() {
        return SqlValue::Null;
    }
    let kind = raw.type_info().name().to_ascii_uppercase();
    match kind.as_str() {
        "INTEGER" | "INT" | "BIGINT" | "BOOLEAN" => row
            .try_get::<i64, _>(idx)
            .map(SqlValue::Int)
            .unwrap_or(SqlValue::Null),
        "REAL" | "FLOAT" | "DOUBLE" => row
            .try_get::<f64, _>(idx)
            .map(SqlValue::Real)
            .unwrap_or(SqlValue::Null),
        "BLOB" => row
            .try_get::<Vec<u8>, _>(idx)
            .map(SqlValue::Blob)
            .unwrap_or(SqlValue::Null),
        _ => row
            .try_get::<String, _>(idx)
            .map(SqlValue::Text)
            .or_else(|_| row.try_get::<i64, _>(idx).map(SqlValue::Int))
            .or_else(|_| row.try_get::<f64, _>(idx).map(SqlValue::Real))
            .unwrap_or(SqlValue::Null),
    }
}

pub(super) async fn table_columns(pool: &SqlitePool, table: &str) -> Result<Vec<String>> {
    use sqlx::Row;
    let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
    Ok(rows
        .iter()
        .filter_map(|r| r.try_get::<String, _>("name").ok())
        .collect())
}

pub(super) type TableRows =
    std::collections::HashMap<String, (Option<DateTime<Utc>>, Vec<SqlValue>)>;

pub(super) async fn read_table(
    pool: &SqlitePool,
    table: &str,
    cols: &[String],
) -> Result<TableRows> {
    use sqlx::Row;
    let list = cols
        .iter()
        .map(|c| format!("\"{c}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let rows = sqlx::query(&format!("SELECT {list} FROM {table}"))
        .fetch_all(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
    let id_idx = cols.iter().position(|c| c == "id");
    let ts_idx = cols.iter().position(|c| c == "updated_at");
    let mut out = TableRows::new();
    for row in rows {
        let Some(id) = id_idx.and_then(|i| row.try_get::<String, _>(i).ok()) else {
            continue;
        };
        let ts = ts_idx
            .and_then(|i| row.try_get::<String, _>(i).ok())
            .and_then(|raw| crate::store::parse_db_timestamp(&raw));
        let values = (0..cols.len()).map(|i| read_value(&row, i)).collect();
        out.insert(id, (ts, values));
    }
    Ok(out)
}

/// Copy every row of `table` that is newer in `from` (or missing in `to`).
/// Returns `(copied, skipped)`. Ties and older rows are left alone, so the
/// newest `updated_at` wins on both sides after a push + pull.
pub(super) async fn merge_table(
    from: &SqlitePool,
    to: &SqlitePool,
    table: &str,
) -> Result<(u64, u64)> {
    let from_cols = table_columns(from, table).await?;
    let to_cols = table_columns(to, table).await?;
    let cols: Vec<String> = from_cols
        .into_iter()
        .filter(|c| to_cols.contains(c))
        .collect();
    if !cols.iter().any(|c| c == "id") {
        return Ok((0, 0));
    }
    let source = read_table(from, table, &cols).await?;
    let target = read_table(to, table, &cols).await?;

    let blanked: Vec<usize> = cols
        .iter()
        .enumerate()
        .filter(|(_, c)| NEVER_PUSHED.contains(&(table, c.as_str())))
        .map(|(i, _)| i)
        .collect();

    let quoted: Vec<String> = cols.iter().map(|c| format!("\"{c}\"")).collect();
    let placeholders = vec!["?"; cols.len()].join(", ");
    let updates = quoted
        .iter()
        .filter(|c| c.as_str() != "\"id\"")
        .map(|c| format!("{c} = excluded.{c}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "INSERT INTO {table} ({}) VALUES ({placeholders}) ON CONFLICT(id) DO UPDATE SET {updates}",
        quoted.join(", ")
    );

    let mut copied = 0;
    let mut skipped = 0;
    let mut tx = to
        .begin()
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
    for (id, (ts, values)) in &source {
        let newer = match target.get(id) {
            None => true,
            Some((theirs, _)) => match (ts, theirs) {
                (Some(ours), Some(theirs)) => ours > theirs,
                (Some(_), None) => true,
                _ => false,
            },
        };
        if !newer {
            skipped += 1;
            continue;
        }
        let mut query = sqlx::query(&sql);
        for (i, value) in values.iter().enumerate() {
            let value = if blanked.contains(&i) {
                &SqlValue::Null
            } else {
                value
            };
            query = match value {
                SqlValue::Null => query.bind(None::<String>),
                SqlValue::Int(v) => query.bind(*v),
                SqlValue::Real(v) => query.bind(*v),
                SqlValue::Text(v) => query.bind(v.clone()),
                SqlValue::Blob(v) => query.bind(v.clone()),
            };
        }
        query
            .execute(&mut *tx)
            .await
            .map_err(|e| Error::DatabaseError(format!("sync {table} row {id}: {e}")))?;
        copied += 1;
    }
    tx.commit()
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
    Ok((copied, skipped))
}
