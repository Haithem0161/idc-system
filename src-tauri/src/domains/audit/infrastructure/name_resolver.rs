//! Resolves the opaque UUIDs in audit rows to human-readable names so the
//! audit table can show "Asma · Dr Apple" instead of `019ecc92 · 4f3a...`.
//!
//! Resolution is BATCHED: for a page of rows we collect the distinct actor ids
//! and the distinct `(entity, entity_id)` pairs, then issue one `SELECT id,
//! name` per referenced table (`WHERE id IN (...)`), and build lookup maps. No
//! per-row query, so a 50-row page costs at most ~6 small reads.
//!
//! Names come straight from local SQLite (the same tables the rest of the app
//! reads). A name that cannot be resolved -- the row was hard-deleted, never
//! synced, or the entity type has no name column -- yields `None`, and the
//! frontend falls back to the short id.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use sqlx::{Row, SqlitePool};

use crate::domains::sync::domain::entities::AuditEntry;
use crate::error::{AppError, AppResult};

/// The zero-UUID sentinel used for system (daemon) actors and system events.
const ZERO_UUID: &str = "00000000-0000-0000-0000-000000000000";

/// Maps an audit `entity` string to the table + name-expression used to resolve
/// its `entity_id` to a display label. `None` for entity types with no
/// resolvable per-row name (settings, junction/log tables, etc.).
fn entity_name_source(entity: &str) -> Option<(&'static str, &'static str)> {
    // (table, name_expr). name_expr is interpolated into SQL but is a fixed
    // string literal from this match -- never user input.
    match entity {
        "users" => Some(("users", "name")),
        "doctors" => Some(("doctors", "name")),
        "operators" => Some(("operators", "name")),
        "patients" => Some(("patients", "name")),
        // Bilingual catalog: prefer English, fall back to Arabic.
        "check_types" => Some(("check_types", "COALESCE(name_en, name_ar)")),
        "check_subtypes" => Some(("check_subtypes", "COALESCE(name_en, name_ar)")),
        "inventory_items" => Some(("inventory_items", "name")),
        _ => None,
    }
}

pub struct NameResolver {
    pool: SqlitePool,
}

/// Resolved name lookups for one page of audit rows.
pub struct ResolvedNames {
    /// actor_user_id -> user name (or "System" for the zero UUID).
    pub actors: HashMap<String, String>,
    /// "<entity>:<entity_id>" -> display label.
    pub entities: HashMap<String, String>,
}

impl ResolvedNames {
    pub fn actor_name(&self, actor_user_id: &str) -> Option<String> {
        self.actors.get(actor_user_id).cloned()
    }
    pub fn entity_label(&self, entity: &str, entity_id: &str) -> Option<String> {
        self.entities.get(&format!("{entity}:{entity_id}")).cloned()
    }
}

impl NameResolver {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Batch-resolve names for a page of audit entries.
    pub async fn resolve(&self, rows: &[AuditEntry]) -> AppResult<ResolvedNames> {
        let mut actors = HashMap::new();
        let mut entities = HashMap::new();

        // ---- actors: distinct user ids -> users.name -------------------
        let actor_ids: BTreeSet<String> = rows
            .iter()
            .map(|r| r.actor_user_id.to_string())
            .filter(|id| id != ZERO_UUID)
            .collect();
        if !actor_ids.is_empty() {
            let names = self.lookup_names("users", "name", &actor_ids).await?;
            actors.extend(names);
        }
        // The daemon actor always reads as "System".
        actors.insert(ZERO_UUID.to_string(), "System".to_string());

        // ---- entities: group ids by table, one query per table ---------
        // entity-type -> set of entity_ids referenced by this page.
        let mut by_entity: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
        for r in rows {
            // The zero-UUID entity_id is a system-event sentinel, not a row.
            if r.entity_id == ZERO_UUID {
                entities.insert(
                    format!("{}:{}", r.entity, r.entity_id),
                    "System".to_string(),
                );
                continue;
            }
            if let Some((_table, _expr)) = entity_name_source(&r.entity) {
                by_entity
                    .entry(r.entity.as_str())
                    .or_default()
                    .insert(r.entity_id.clone());
            }
        }
        for (entity, ids) in by_entity {
            let (table, expr) =
                entity_name_source(entity).expect("by_entity only holds resolvable entities");
            let names = self.lookup_names(table, expr, &ids).await?;
            for (id, name) in names {
                entities.insert(format!("{entity}:{id}"), name);
            }
        }

        Ok(ResolvedNames { actors, entities })
    }

    /// `SELECT id, <name_expr> FROM <table> WHERE id IN (?, ?, ...)`. `table`
    /// and `name_expr` are fixed literals from `entity_name_source`, never user
    /// input; only the ids are bound. Soft-deleted rows are still resolved (the
    /// audit history should name a doctor even after they were archived).
    async fn lookup_names(
        &self,
        table: &str,
        name_expr: &str,
        ids: &BTreeSet<String>,
    ) -> AppResult<HashMap<String, String>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let placeholders = vec!["?"; ids.len()].join(", ");
        let sql = format!(
            "SELECT id, {name_expr} AS display_name FROM {table} WHERE id IN ({placeholders})"
        );
        let mut q = sqlx::query(&sql);
        for id in ids {
            q = q.bind(id);
        }
        let fetched = q.fetch_all(&self.pool).await.map_err(AppError::from)?;
        let mut out = HashMap::with_capacity(fetched.len());
        for row in fetched {
            let id: String = row.get("id");
            let name: Option<String> = row.get("display_name");
            if let Some(name) = name {
                out.insert(id, name);
            }
        }
        Ok(out)
    }
}
