//! App-data workspace persistence (open projects + chats).
//! Separate from project-local index DBs and settings.sqlite.

use crate::normalize_root_key;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub fn workspace_db_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("workspace.sqlite")
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceSnapshot {
    pub active_root_key: Option<String>,
    pub open_projects: Vec<PersistedOpenProject>,
    pub chats: Vec<PersistedChat>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedOpenProject {
    pub root_key: String,
    pub root_path: String,
    pub sort_order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedChat {
    pub id: String,
    pub root_key: String,
    pub title: String,
    pub composer_mode: String,
    pub sort_order: i64,
    pub active: bool,
    pub messages: Vec<PersistedMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedMessage {
    pub id: String,
    pub role: String,
    pub content_json: String,
    pub sort_order: i64,
}

pub fn migrate_workspace(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS open_projects (
                root_key TEXT PRIMARY KEY,
                root_path TEXT NOT NULL,
                sort_order INTEGER NOT NULL,
                opened_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS workspace_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS chats (
                id TEXT PRIMARY KEY,
                root_key TEXT NOT NULL,
                title TEXT NOT NULL,
                composer_mode TEXT NOT NULL,
                sort_order INTEGER NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                chat_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL,
                sort_order INTEGER NOT NULL,
                FOREIGN KEY(chat_id) REFERENCES chats(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_chats_root ON chats(root_key);
            CREATE INDEX IF NOT EXISTS idx_messages_chat ON messages(chat_id);
            ",
        )
        .map_err(|e| format!("workspace migrate: {e}"))
}

pub fn open_workspace_db(db_path: &Path) -> Result<Connection, String> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create workspace dir: {e}"))?;
    }
    let connection = Connection::open(db_path).map_err(|e| format!("open workspace db: {e}"))?;
    migrate_workspace(&connection)?;
    Ok(connection)
}

pub fn load_workspace_snapshot(db_path: &Path) -> Result<WorkspaceSnapshot, String> {
    if !db_path.exists() {
        return Ok(WorkspaceSnapshot {
            active_root_key: None,
            open_projects: vec![],
            chats: vec![],
        });
    }
    let connection = open_workspace_db(db_path)?;

    let mut open_projects = Vec::new();
    {
        let mut stmt = connection
            .prepare(
                "SELECT root_key, root_path, sort_order FROM open_projects ORDER BY sort_order ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(PersistedOpenProject {
                    root_key: row.get(0)?,
                    root_path: row.get(1)?,
                    sort_order: row.get(2)?,
                })
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            open_projects.push(row.map_err(|e| e.to_string())?);
        }
    }

    let active_root_key: Option<String> = connection
        .query_row(
            "SELECT value FROM workspace_meta WHERE key = 'active_root_key'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;

    let mut active_by_root: HashMap<String, String> = HashMap::new();
    {
        let mut stmt = connection
            .prepare("SELECT key, value FROM workspace_meta WHERE key LIKE 'active_chat:%'")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (key, value) = row.map_err(|e| e.to_string())?;
            if let Some(root_key) = key.strip_prefix("active_chat:") {
                active_by_root.insert(root_key.to_string(), value);
            }
        }
    }

    let mut chats = Vec::new();
    {
        let mut stmt = connection
            .prepare(
                "SELECT id, root_key, title, composer_mode, sort_order
                 FROM chats ORDER BY root_key ASC, sort_order ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })
            .map_err(|e| e.to_string())?;

        for row in rows {
            let (id, root_key, title, composer_mode, sort_order) = row.map_err(|e| e.to_string())?;
            let messages = load_messages(&connection, &id)?;
            let active = active_by_root.get(&root_key).map(|v| v == &id).unwrap_or(false);
            chats.push(PersistedChat {
                id,
                root_key,
                title,
                composer_mode,
                sort_order,
                active,
                messages,
            });
        }
    }

    // If no active_chat meta, mark first chat per root as active for UI convenience.
    let mut seen_roots = HashSet::new();
    for chat in &mut chats {
        if chat.active {
            seen_roots.insert(chat.root_key.clone());
        }
    }
    for chat in &mut chats {
        if !seen_roots.contains(&chat.root_key) {
            chat.active = true;
            seen_roots.insert(chat.root_key.clone());
        }
    }

    Ok(WorkspaceSnapshot {
        active_root_key,
        open_projects,
        chats,
    })
}

/// Load all chats for a root_key (including when project is not in open_projects).
pub fn load_chats_for_root_key(
    db_path: &Path,
    root_key: &str,
) -> Result<Vec<PersistedChat>, String> {
    if !db_path.exists() {
        return Ok(vec![]);
    }
    let snap = load_workspace_snapshot(db_path)?;
    Ok(snap
        .chats
        .into_iter()
        .filter(|c| c.root_key == root_key)
        .collect())
}

fn load_messages(connection: &Connection, chat_id: &str) -> Result<Vec<PersistedMessage>, String> {
    let mut stmt = connection
        .prepare(
            "SELECT id, role, content, sort_order FROM messages
             WHERE chat_id = ?1 ORDER BY sort_order ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![chat_id], |row| {
            Ok(PersistedMessage {
                id: row.get(0)?,
                role: row.get(1)?,
                content_json: row.get(2)?,
                sort_order: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut messages = Vec::new();
    for row in rows {
        messages.push(row.map_err(|e| e.to_string())?);
    }
    Ok(messages)
}

pub fn save_workspace_snapshot(db_path: &Path, snapshot: &WorkspaceSnapshot) -> Result<(), String> {
    let mut connection = open_workspace_db(db_path)?;
    let tx = connection
        .transaction()
        .map_err(|e| format!("workspace tx: {e}"))?;

    tx.execute("DELETE FROM open_projects", [])
        .map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM workspace_meta WHERE key = 'active_root_key'", [])
        .map_err(|e| e.to_string())?;

    let now = Utc::now().to_rfc3339();
    for project in &snapshot.open_projects {
        let root_key = if project.root_key.is_empty() {
            normalize_root_key(&project.root_path)
        } else {
            project.root_key.clone()
        };
        tx.execute(
            "INSERT INTO open_projects(root_key, root_path, sort_order, opened_at)
             VALUES(?1, ?2, ?3, ?4)",
            params![root_key, project.root_path, project.sort_order, now],
        )
        .map_err(|e| e.to_string())?;
    }

    if let Some(active) = &snapshot.active_root_key {
        tx.execute(
            "INSERT INTO workspace_meta(key, value) VALUES('active_root_key', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![active],
        )
        .map_err(|e| e.to_string())?;
    }

    // Replace chats only for root_keys present in snapshot.chats (keeps other roots untouched).
    let mut roots: Vec<String> = snapshot
        .chats
        .iter()
        .map(|c| c.root_key.clone())
        .collect();
    roots.sort();
    roots.dedup();

    for root_key in &roots {
        // Delete messages for chats under this root, then chats.
        tx.execute(
            "DELETE FROM messages WHERE chat_id IN (SELECT id FROM chats WHERE root_key = ?1)",
            params![root_key],
        )
        .map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM chats WHERE root_key = ?1", params![root_key])
            .map_err(|e| e.to_string())?;
        tx.execute(
            "DELETE FROM workspace_meta WHERE key = ?1",
            params![format!("active_chat:{root_key}")],
        )
        .map_err(|e| e.to_string())?;
    }

    for chat in &snapshot.chats {
        tx.execute(
            "INSERT INTO chats(id, root_key, title, composer_mode, sort_order, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                chat.id,
                chat.root_key,
                chat.title,
                chat.composer_mode,
                chat.sort_order,
                now
            ],
        )
        .map_err(|e| e.to_string())?;

        for message in &chat.messages {
            tx.execute(
                "INSERT INTO messages(id, chat_id, role, content, created_at, sort_order)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    message.id,
                    chat.id,
                    message.role,
                    message.content_json,
                    now,
                    message.sort_order
                ],
            )
            .map_err(|e| e.to_string())?;
        }

        if chat.active {
            tx.execute(
                "INSERT INTO workspace_meta(key, value) VALUES(?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![format!("active_chat:{}", chat.root_key), chat.id],
            )
            .map_err(|e| e.to_string())?;
        }
    }

    tx.commit().map_err(|e| format!("workspace commit: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_round_trip_keeps_chats_after_open_project_removed() {
        let dir = std::env::temp_dir().join(format!("cppinsight-ws-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = workspace_db_path(&dir);

        let root_path = r"D:\repos\demo";
        let root_key = normalize_root_key(root_path);
        let snap = WorkspaceSnapshot {
            active_root_key: Some(root_key.clone()),
            open_projects: vec![PersistedOpenProject {
                root_key: root_key.clone(),
                root_path: root_path.to_string(),
                sort_order: 0,
            }],
            chats: vec![PersistedChat {
                id: "c1".into(),
                root_key: root_key.clone(),
                title: "对话 1".into(),
                composer_mode: "ask".into(),
                sort_order: 0,
                active: true,
                messages: vec![PersistedMessage {
                    id: "m1".into(),
                    role: "user".into(),
                    content_json: r#"{"id":"m1","role":"user","text":"入口在哪","kind":"text"}"#
                        .into(),
                    sort_order: 0,
                }],
            }],
        };
        save_workspace_snapshot(&db, &snap).expect("save");
        let loaded = load_workspace_snapshot(&db).expect("load");
        assert_eq!(loaded.open_projects.len(), 1);
        assert_eq!(loaded.chats[0].messages.len(), 1);

        let closed = WorkspaceSnapshot {
            active_root_key: None,
            open_projects: vec![],
            chats: snap.chats.clone(),
        };
        save_workspace_snapshot(&db, &closed).expect("save closed");
        let after = load_workspace_snapshot(&db).expect("load after close");
        assert!(after.open_projects.is_empty());
        assert_eq!(after.chats.len(), 1);
        assert_eq!(after.chats[0].messages[0].id, "m1");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
