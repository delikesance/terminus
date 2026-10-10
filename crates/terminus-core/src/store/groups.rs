use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use super::{row_ts, row_uuid, Store};
use crate::error::{Error, Result};
use crate::models::*;

impl Store {
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
            .filter_map(|r| -> Option<Group> {
                Some(Group {
                    id: row_uuid(&r, "id")?,
                    name: r.try_get("name").ok()?,
                    parent_id: r
                        .try_get::<Option<String>, _>("parent_id")
                        .ok()
                        .flatten()
                        .and_then(|s| Uuid::parse_str(&s).ok()),
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

        // Rows whose position changes get a fresh `updated_at` so the new
        // order wins on other devices after a sync.
        let now = Utc::now();
        for (i, entry) in root.into_iter().enumerate() {
            match entry {
                RootEntry::Host(mut h) => {
                    if h.sort_order != i as i64 {
                        h.sort_order = i as i64;
                        h.updated_at = now;
                    }
                    self.upsert_host(&h).await?;
                }
                RootEntry::Group(mut g) => {
                    if g.sort_order != i as i64 {
                        g.sort_order = i as i64;
                        g.updated_at = now;
                    }
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
        hosts.sort_by_key(|a| a.name.to_lowercase());
        let mut groups: Vec<Group> = self
            .list_groups()
            .await?
            .into_iter()
            .filter(|g| g.deleted_at.is_none())
            .collect();
        groups.sort_by_key(|a| a.name.to_lowercase());

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
}
