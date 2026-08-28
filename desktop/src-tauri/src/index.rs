use crate::{build_chunks, sql_error, ScanResult, SymbolInfo};
use chrono::Utc;
use rusqlite::{params, Connection};

pub const INDEX_SCHEMA_VERSION: u32 = 3;

pub fn write_index(connection: &mut Connection, scan: &ScanResult) -> Result<i64, String> {
    let transaction = connection.transaction().map_err(sql_error)?;
    let indexed_at = Utc::now().to_rfc3339();
    let diagnostics_json = serde_json::to_string(&scan.diagnostics)
        .map_err(|error| format!("failed to encode diagnostics: {error}"))?;
    let mode = if scan.mode.is_empty() {
        "heuristic"
    } else {
        scan.mode.as_str()
    };
    let confidence = if scan.confidence.is_empty() {
        "heuristic"
    } else {
        scan.confidence.as_str()
    };

    transaction
        .execute(
            "INSERT INTO projects(root, indexed_at, file_count, symbol_count, include_count, diagnostics_json, mode, confidence)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(root) DO UPDATE SET
                indexed_at = excluded.indexed_at,
                file_count = excluded.file_count,
                symbol_count = excluded.symbol_count,
                include_count = excluded.include_count,
                diagnostics_json = excluded.diagnostics_json,
                mode = excluded.mode,
                confidence = excluded.confidence",
            params![
                scan.root,
                indexed_at,
                scan.files.len() as i64,
                scan.symbols.len() as i64,
                scan.includes.len() as i64,
                diagnostics_json,
                mode,
                confidence
            ],
        )
        .map_err(sql_error)?;

    let project_id: i64 = transaction
        .query_row(
            "SELECT id FROM projects WHERE root = ?1",
            params![scan.root],
            |row| row.get(0),
        )
        .map_err(sql_error)?;

    let next_generation = transaction
        .query_row(
            "SELECT index_generation + 1 FROM projects WHERE id = ?1",
            params![project_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(sql_error)?;

    transaction
        .execute(
            "DELETE FROM files WHERE project_id = ?1",
            params![project_id],
        )
        .map_err(sql_error)?;
    transaction
        .execute(
            "DELETE FROM symbols WHERE project_id = ?1",
            params![project_id],
        )
        .map_err(sql_error)?;
    transaction
        .execute(
            "DELETE FROM includes WHERE project_id = ?1",
            params![project_id],
        )
        .map_err(sql_error)?;
    transaction
        .execute(
            "DELETE FROM calls WHERE project_id = ?1",
            params![project_id],
        )
        .map_err(sql_error)?;
    transaction
        .execute(
            "DELETE FROM symbol_relations WHERE project_id = ?1",
            params![project_id],
        )
        .map_err(sql_error)?;
    transaction
        .execute(
            "DELETE FROM module_relations WHERE project_id = ?1",
            params![project_id],
        )
        .map_err(sql_error)?;
    transaction
        .execute(
            "DELETE FROM entry_points WHERE project_id = ?1",
            params![project_id],
        )
        .map_err(sql_error)?;
    transaction
        .execute(
            "DELETE FROM chunks WHERE project_id = ?1",
            params![project_id],
        )
        .map_err(sql_error)?;

    for file in &scan.files {
        transaction
            .execute(
                "INSERT INTO files(project_id, path, extension, category, size_bytes, total_lines, code_lines, comment_lines, blank_lines, mtime_unix)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    project_id,
                    file.path,
                    file.extension,
                    file.category,
                    file.size_bytes as i64,
                    file.lines.total as i64,
                    file.lines.code as i64,
                    file.lines.comment as i64,
                    file.lines.blank as i64,
                    file.mtime_unix as i64,
                ],
            )
            .map_err(sql_error)?;
    }

    for symbol in &scan.symbols {
        let stable_key = stable_symbol_key(symbol);
        let qualified_name = normalized_qualified_name(symbol);
        transaction
            .execute(
                "INSERT INTO symbols(
                    project_id, file, line, kind, name, qualified_name, stable_key,
                    parent_qualified_name, parent_stable_key, signature, scope,
                    is_definition, confidence
                 ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    project_id,
                    symbol.file,
                    symbol.line as i64,
                    symbol.kind,
                    symbol.name,
                    qualified_name,
                    stable_key,
                    symbol.parent_qualified_name,
                    symbol.parent_stable_key,
                    symbol.signature,
                    symbol.scope,
                    if symbol.is_definition { 1 } else { 0 },
                    symbol.confidence,
                ],
            )
            .map_err(sql_error)?;
    }

    for include in &scan.includes {
        transaction
            .execute(
                "INSERT INTO includes(project_id, file, line, target, kind, resolved, resolved_path)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    project_id,
                    include.file,
                    include.line as i64,
                    include.target,
                    include.kind,
                    if include.resolved { 1 } else { 0 },
                    include.resolved_path
                ],
            )
            .map_err(sql_error)?;
    }

    for call in &scan.calls {
        transaction
            .execute(
                "INSERT INTO calls(
                    project_id, file, line, name, target_qualified_name,
                    caller_qualified_name, caller_stable_key, confidence
                 ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    project_id,
                    call.file,
                    call.line as i64,
                    call.name,
                    call.target_qualified_name,
                    call.caller_qualified_name,
                    call.caller_stable_key,
                    call.confidence,
                ],
            )
            .map_err(sql_error)?;
    }

    for relation in &scan.relations {
        transaction
            .execute(
                "INSERT INTO symbol_relations(
                    project_id, source_key, target_name, relation_kind,
                    evidence_file, evidence_line, confidence
                 ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    project_id,
                    relation.source_stable_key,
                    relation.target_name,
                    relation.kind,
                    relation.file,
                    relation.line as i64,
                    relation.confidence,
                ],
            )
            .map_err(sql_error)?;
    }

    for entry in &scan.entry_points {
        transaction
            .execute(
                "INSERT INTO entry_points(
                    project_id, symbol_key, qualified_name, entry_kind,
                    evidence_file, evidence_line, confidence
                 ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    project_id,
                    entry.symbol_stable_key,
                    entry.qualified_name,
                    entry.kind,
                    entry.file,
                    entry.line as i64,
                    entry.confidence,
                ],
            )
            .map_err(sql_error)?;
    }

    for chunk in build_chunks(scan)? {
        transaction
            .execute(
                "INSERT INTO chunks(project_id, file, start_line, end_line, text)
                 VALUES(?1, ?2, ?3, ?4, ?5)",
                params![
                    project_id,
                    chunk.file,
                    chunk.start_line as i64,
                    chunk.end_line as i64,
                    chunk.text
                ],
            )
            .map_err(sql_error)?;
    }

    transaction
        .execute(
            "UPDATE projects
             SET index_schema_version = ?2,
                 generator_version = ?3,
                 index_generation = ?4,
                 index_complete = 1
             WHERE id = ?1",
            params![
                project_id,
                INDEX_SCHEMA_VERSION as i64,
                generator_version(scan),
                next_generation,
            ],
        )
        .map_err(sql_error)?;

    transaction.commit().map_err(sql_error)?;
    Ok(project_id)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexStatus {
    pub schema_version: u32,
    pub generator_version: String,
    pub generation: u64,
    pub complete: bool,
}

fn column_exists(connection: &Connection, table: &str, column: &str) -> Result<bool, String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(sql_error)?;
    for row in rows {
        if row.map_err(sql_error)? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn add_column(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), String> {
    if column_exists(connection, table, column)? {
        return Ok(());
    }
    connection
        .execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {definition};"
        ))
        .map_err(sql_error)
}

pub(crate) fn migrate_legacy_base_schema(connection: &Connection) -> Result<(), String> {
    for (table, column, definition) in [
        ("projects", "mode", "TEXT NOT NULL DEFAULT 'heuristic'"),
        (
            "projects",
            "confidence",
            "TEXT NOT NULL DEFAULT 'heuristic'",
        ),
        ("files", "mtime_unix", "INTEGER NOT NULL DEFAULT 0"),
        ("symbols", "confidence", "TEXT NOT NULL DEFAULT 'heuristic'"),
        ("includes", "resolved", "INTEGER NOT NULL DEFAULT 0"),
        ("includes", "resolved_path", "TEXT NOT NULL DEFAULT ''"),
        ("calls", "confidence", "TEXT NOT NULL DEFAULT 'heuristic'"),
    ] {
        add_column(connection, table, column, definition)?;
    }
    connection
        .execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_symbols_project_name
                 ON symbols(project_id, name);
             CREATE INDEX IF NOT EXISTS idx_calls_project_name
                 ON calls(project_id, name);",
        )
        .map_err(sql_error)
}

pub(crate) fn migrate_graph_schema(connection: &Connection) -> Result<(), String> {
    for (table, column, definition) in [
        (
            "projects",
            "index_schema_version",
            "INTEGER NOT NULL DEFAULT 2",
        ),
        (
            "projects",
            "generator_version",
            "TEXT NOT NULL DEFAULT 'legacy'",
        ),
        ("projects", "index_generation", "INTEGER NOT NULL DEFAULT 0"),
        ("projects", "index_complete", "INTEGER NOT NULL DEFAULT 1"),
        ("symbols", "stable_key", "TEXT NOT NULL DEFAULT ''"),
        ("symbols", "qualified_name", "TEXT NOT NULL DEFAULT ''"),
        ("symbols", "parent_stable_key", "TEXT NOT NULL DEFAULT ''"),
        (
            "symbols",
            "parent_qualified_name",
            "TEXT NOT NULL DEFAULT ''",
        ),
        ("calls", "caller_stable_key", "TEXT NOT NULL DEFAULT ''"),
        ("calls", "caller_qualified_name", "TEXT NOT NULL DEFAULT ''"),
        ("calls", "target_qualified_name", "TEXT NOT NULL DEFAULT ''"),
        ("calls", "resolved_target_key", "TEXT NOT NULL DEFAULT ''"),
        (
            "calls",
            "candidate_target_keys_json",
            "TEXT NOT NULL DEFAULT '[]'",
        ),
    ] {
        add_column(connection, table, column, definition)?;
    }

    connection
        .execute_batch(
            "
            CREATE TABLE IF NOT EXISTS symbol_relations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_id INTEGER NOT NULL,
                source_key TEXT NOT NULL,
                target_key TEXT NOT NULL DEFAULT '',
                target_name TEXT NOT NULL,
                relation_kind TEXT NOT NULL,
                evidence_file TEXT NOT NULL,
                evidence_line INTEGER NOT NULL,
                confidence TEXT NOT NULL,
                ambiguous INTEGER NOT NULL DEFAULT 0,
                candidate_target_keys_json TEXT NOT NULL DEFAULT '[]',
                FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS module_relations (
                project_id INTEGER NOT NULL,
                source_module TEXT NOT NULL,
                target_module TEXT NOT NULL,
                relation_kind TEXT NOT NULL,
                weight INTEGER NOT NULL,
                confidence TEXT NOT NULL,
                evidence_count INTEGER NOT NULL,
                PRIMARY KEY(project_id, source_module, target_module, relation_kind),
                FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS graph_layouts (
                project_id INTEGER NOT NULL,
                graph_kind TEXT NOT NULL,
                view_key TEXT NOT NULL,
                protocol_version INTEGER NOT NULL,
                positions_json TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY(project_id, graph_kind, view_key),
                FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS entry_points (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_id INTEGER NOT NULL,
                symbol_key TEXT NOT NULL,
                qualified_name TEXT NOT NULL,
                entry_kind TEXT NOT NULL,
                evidence_file TEXT NOT NULL,
                evidence_line INTEGER NOT NULL,
                confidence TEXT NOT NULL,
                FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_symbols_project_stable_key
                ON symbols(project_id, stable_key);
            CREATE INDEX IF NOT EXISTS idx_symbols_project_qualified_name
                ON symbols(project_id, qualified_name);
            CREATE INDEX IF NOT EXISTS idx_calls_project_caller
                ON calls(project_id, caller_stable_key);
            CREATE INDEX IF NOT EXISTS idx_calls_project_target
                ON calls(project_id, resolved_target_key);
            CREATE INDEX IF NOT EXISTS idx_symbol_relations_project_source
                ON symbol_relations(project_id, source_key, relation_kind);
            CREATE INDEX IF NOT EXISTS idx_symbol_relations_project_target
                ON symbol_relations(project_id, target_key, relation_kind);
            CREATE INDEX IF NOT EXISTS idx_entry_points_project_symbol
                ON entry_points(project_id, symbol_key);
            ",
        )
        .map_err(sql_error)
}

pub fn load_index_status(connection: &Connection, project_id: i64) -> Result<IndexStatus, String> {
    connection
        .query_row(
            "SELECT index_schema_version, generator_version, index_generation, index_complete
             FROM projects WHERE id = ?1",
            params![project_id],
            |row| {
                Ok(IndexStatus {
                    schema_version: row.get::<_, i64>(0)? as u32,
                    generator_version: row.get(1)?,
                    generation: row.get::<_, i64>(2)? as u64,
                    complete: row.get::<_, i64>(3)? != 0,
                })
            },
        )
        .map_err(sql_error)
}

pub(crate) fn generator_version(scan: &ScanResult) -> String {
    if !scan.generator_version.trim().is_empty() {
        return scan.generator_version.clone();
    }
    format!("legacy-json/{}", scan.schema_version)
}

fn normalize_signature(signature: &str) -> String {
    signature.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn qualified_name(symbol: &SymbolInfo) -> String {
    if !symbol.qualified_name.trim().is_empty() {
        return symbol.qualified_name.clone();
    }
    if symbol.scope.trim().is_empty() {
        symbol.name.clone()
    } else {
        format!("{}::{}", symbol.scope, symbol.name)
    }
}

pub fn stable_symbol_key(symbol: &SymbolInfo) -> String {
    if !symbol.stable_key.trim().is_empty() {
        return symbol.stable_key.clone();
    }
    let file = symbol.file.replace('\\', "/");
    let canonical = format!(
        "{}\n{}\n{}\n{}",
        symbol.kind,
        file,
        qualified_name(symbol),
        normalize_signature(&symbol.signature)
    );
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in canonical.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("sym:{hash:016x}")
}

pub(crate) fn normalized_qualified_name(symbol: &SymbolInfo) -> String {
    qualified_name(symbol)
}

pub(crate) fn upgrade_legacy_project_rows(
    connection: &Connection,
    project_id: i64,
) -> Result<(), String> {
    let symbols = {
        let mut statement = connection
            .prepare(
                "SELECT id, file, line, kind, name, qualified_name, stable_key,
                        parent_qualified_name, parent_stable_key, signature, scope,
                        is_definition, confidence
                 FROM symbols WHERE project_id = ?1 ORDER BY id",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![project_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    SymbolInfo {
                        file: row.get(1)?,
                        line: row.get::<_, i64>(2)? as u64,
                        kind: row.get(3)?,
                        name: row.get(4)?,
                        qualified_name: row.get(5)?,
                        stable_key: row.get(6)?,
                        parent_qualified_name: row.get(7)?,
                        parent_stable_key: row.get(8)?,
                        signature: row.get(9)?,
                        scope: row.get(10)?,
                        is_definition: row.get::<_, i64>(11)? != 0,
                        confidence: row.get(12)?,
                    },
                ))
            })
            .map_err(sql_error)?;
        let mut collected = Vec::new();
        for row in rows {
            collected.push(row.map_err(sql_error)?);
        }
        collected
    };

    for (row_id, symbol) in symbols {
        connection
            .execute(
                "UPDATE symbols
                 SET stable_key = ?2, qualified_name = ?3
                 WHERE id = ?1",
                params![
                    row_id,
                    stable_symbol_key(&symbol),
                    normalized_qualified_name(&symbol)
                ],
            )
            .map_err(sql_error)?;
    }

    connection
        .execute(
            "UPDATE projects
             SET index_schema_version = ?2,
                 generator_version = CASE
                     WHEN index_schema_version >= ?2 AND generator_version != 'legacy'
                         THEN generator_version
                     ELSE 'legacy-db'
                 END,
                 index_generation = CASE
                     WHEN index_generation < 1 THEN 1
                     ELSE index_generation
                 END,
                 index_complete = 1
             WHERE id = ?1",
            params![project_id, INDEX_SCHEMA_VERSION as i64],
        )
        .map_err(sql_error)?;
    Ok(())
}
