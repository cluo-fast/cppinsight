use crate::{build_chunks, sql_error, ScanResult, SymbolInfo};
use chrono::Utc;
use rusqlite::{params, Connection};
use std::collections::HashMap;

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

    resolve_graph_facts(&transaction, project_id)?;

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

#[derive(Debug, Clone)]
struct StoredSymbol {
    stable_key: String,
    qualified_name: String,
    name: String,
    file: String,
    line: u64,
    is_definition: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolutionSummary {
    pub resolved: usize,
    pub ambiguous: usize,
    pub unresolved: usize,
    pub module_relations: usize,
}

fn parent_name(qualified_name: &str) -> &str {
    qualified_name
        .rsplit_once("::")
        .map(|(parent, _)| parent)
        .unwrap_or("")
}

fn module_for_file(file: &str) -> String {
    let normalized = file.replace('\\', "/");
    let top = normalized
        .split('/')
        .find(|part| !part.is_empty() && *part != ".")
        .unwrap_or(".");
    format!("module:{top}")
}

fn load_stored_symbols(
    connection: &Connection,
    project_id: i64,
) -> Result<Vec<StoredSymbol>, String> {
    let mut statement = connection
        .prepare(
            "SELECT stable_key, qualified_name, name, file, line, is_definition
             FROM symbols WHERE project_id = ?1
             ORDER BY stable_key, is_definition DESC, file, line",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map(params![project_id], |row| {
            Ok(StoredSymbol {
                stable_key: row.get(0)?,
                qualified_name: row.get(1)?,
                name: row.get(2)?,
                file: row.get(3)?,
                line: row.get::<_, i64>(4)? as u64,
                is_definition: row.get::<_, i64>(5)? != 0,
            })
        })
        .map_err(sql_error)?;
    let mut symbols = Vec::new();
    for row in rows {
        symbols.push(row.map_err(sql_error)?);
    }
    Ok(symbols)
}

fn resolve_candidates(
    symbols: &[StoredSymbol],
    source: Option<&StoredSymbol>,
    target_qualified_name: &str,
    target_name: &str,
) -> Vec<String> {
    let qualified_hint = if !target_qualified_name.trim().is_empty() {
        target_qualified_name.trim()
    } else if target_name.contains("::") {
        target_name.trim()
    } else {
        ""
    };
    let unqualified = target_name
        .trim()
        .rsplit_once("::")
        .map(|(_, name)| name)
        .unwrap_or_else(|| target_name.trim());

    let mut scored = Vec::new();
    for candidate in symbols {
        let base_score = if !qualified_hint.is_empty() && candidate.qualified_name == qualified_hint
        {
            400
        } else if candidate.name != unqualified {
            continue;
        } else if source.is_some_and(|source| {
            !parent_name(source.qualified_name.as_str()).is_empty()
                && parent_name(source.qualified_name.as_str())
                    == parent_name(candidate.qualified_name.as_str())
        }) {
            300
        } else if source.is_some_and(|source| {
            module_for_file(source.file.as_str()) == module_for_file(candidate.file.as_str())
        }) {
            200
        } else {
            100
        };
        let score = base_score + i32::from(candidate.is_definition);
        scored.push((score, candidate.stable_key.clone()));
    }

    let Some(best_score) = scored.iter().map(|(score, _)| *score).max() else {
        return Vec::new();
    };
    let mut candidates = scored
        .into_iter()
        .filter_map(|(score, stable_key)| (score == best_score).then_some(stable_key))
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.dedup();
    candidates
}

fn resolution_state(candidates: &[String]) -> (&'static str, bool, String) {
    match candidates {
        [] => ("unresolved", false, String::new()),
        [target] => ("resolved", false, target.clone()),
        _ => ("ambiguous", true, String::new()),
    }
}

fn aggregate_module_relation(
    connection: &Connection,
    project_id: i64,
    source_module: &str,
    target_module: &str,
    relation_kind: &str,
    confidence: &str,
) -> Result<(), String> {
    if source_module == target_module {
        return Ok(());
    }
    connection
        .execute(
            "INSERT INTO module_relations(
                project_id, source_module, target_module, relation_kind,
                weight, confidence, evidence_count
             ) VALUES(?1, ?2, ?3, ?4, 1, ?5, 1)
             ON CONFLICT(project_id, source_module, target_module, relation_kind)
             DO UPDATE SET
                weight = weight + 1,
                evidence_count = evidence_count + 1,
                confidence = CASE
                    WHEN excluded.confidence = 'unresolved' THEN 'unresolved'
                    WHEN excluded.confidence = 'ambiguous'
                         AND confidence != 'unresolved' THEN 'ambiguous'
                    WHEN excluded.confidence = 'heuristic'
                         AND confidence NOT IN ('unresolved', 'ambiguous') THEN 'heuristic'
                    ELSE confidence
                END",
            params![
                project_id,
                source_module,
                target_module,
                relation_kind,
                confidence
            ],
        )
        .map_err(sql_error)?;
    Ok(())
}

pub fn resolve_graph_facts(
    connection: &Connection,
    project_id: i64,
) -> Result<ResolutionSummary, String> {
    let symbols = load_stored_symbols(connection, project_id)?;
    let by_key: HashMap<&str, &StoredSymbol> = symbols
        .iter()
        .map(|symbol| (symbol.stable_key.as_str(), symbol))
        .collect();
    let by_qualified: HashMap<&str, &StoredSymbol> = symbols
        .iter()
        .filter(|symbol| symbol.is_definition)
        .map(|symbol| (symbol.qualified_name.as_str(), symbol))
        .collect();
    let mut definitions_by_file: HashMap<&str, Vec<&StoredSymbol>> = HashMap::new();
    for symbol in symbols.iter().filter(|symbol| symbol.is_definition) {
        definitions_by_file
            .entry(symbol.file.as_str())
            .or_default()
            .push(symbol);
    }
    for definitions in definitions_by_file.values_mut() {
        definitions.sort_by_key(|symbol| symbol.line);
    }

    connection
        .execute(
            "DELETE FROM symbol_relations
             WHERE project_id = ?1 AND relation_kind = 'calls'",
            params![project_id],
        )
        .map_err(sql_error)?;

    let calls = {
        let mut statement = connection
            .prepare(
                "SELECT id, file, line, name, target_qualified_name,
                        caller_qualified_name, caller_stable_key
                 FROM calls WHERE project_id = ?1 ORDER BY id",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![project_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)? as u64,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })
            .map_err(sql_error)?;
        let mut values = Vec::new();
        for row in rows {
            values.push(row.map_err(sql_error)?);
        }
        values
    };

    let mut summary = ResolutionSummary::default();
    for (id, file, line, name, target_qualified_name, caller_qualified_name, caller_key) in calls {
        let source = by_key
            .get(caller_key.as_str())
            .copied()
            .or_else(|| by_qualified.get(caller_qualified_name.as_str()).copied())
            .or_else(|| {
                definitions_by_file
                    .get(file.as_str())
                    .and_then(|definitions| {
                        definitions
                            .iter()
                            .rev()
                            .find(|symbol| symbol.line <= line)
                            .copied()
                    })
            });
        let candidates =
            resolve_candidates(&symbols, source, &target_qualified_name, name.as_str());
        let (confidence, ambiguous, target_key) = resolution_state(&candidates);
        match confidence {
            "resolved" => summary.resolved += 1,
            "ambiguous" => summary.ambiguous += 1,
            _ => summary.unresolved += 1,
        }
        let candidates_json = serde_json::to_string(&candidates)
            .map_err(|error| format!("failed to encode call candidates: {error}"))?;
        let source_key = source
            .map(|symbol| symbol.stable_key.as_str())
            .unwrap_or(caller_key.as_str());
        let source_name = source
            .map(|symbol| symbol.qualified_name.as_str())
            .unwrap_or(caller_qualified_name.as_str());
        connection
            .execute(
                "UPDATE calls
                 SET caller_stable_key = ?2,
                     caller_qualified_name = ?3,
                     resolved_target_key = ?4,
                     candidate_target_keys_json = ?5,
                     confidence = ?6
                 WHERE id = ?1",
                params![
                    id,
                    source_key,
                    source_name,
                    target_key,
                    candidates_json,
                    confidence
                ],
            )
            .map_err(sql_error)?;
        if !source_key.is_empty() {
            connection
                .execute(
                    "INSERT INTO symbol_relations(
                        project_id, source_key, target_key, target_name, relation_kind,
                        evidence_file, evidence_line, confidence, ambiguous,
                        candidate_target_keys_json
                     ) VALUES(?1, ?2, ?3, ?4, 'calls', ?5, ?6, ?7, ?8, ?9)",
                    params![
                        project_id,
                        source_key,
                        target_key,
                        if target_qualified_name.is_empty() {
                            name.as_str()
                        } else {
                            target_qualified_name.as_str()
                        },
                        file,
                        line as i64,
                        confidence,
                        i64::from(ambiguous),
                        candidates_json,
                    ],
                )
                .map_err(sql_error)?;
        }
    }

    let raw_relations = {
        let mut statement = connection
            .prepare(
                "SELECT id, source_key, target_name
                 FROM symbol_relations
                 WHERE project_id = ?1 AND relation_kind != 'calls'
                 ORDER BY id",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![project_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(sql_error)?;
        let mut values = Vec::new();
        for row in rows {
            values.push(row.map_err(sql_error)?);
        }
        values
    };
    for (id, source_key, target_name) in raw_relations {
        let source = by_key.get(source_key.as_str()).copied();
        let candidates = resolve_candidates(&symbols, source, &target_name, &target_name);
        let (confidence, ambiguous, target_key) = resolution_state(&candidates);
        let candidates_json = serde_json::to_string(&candidates)
            .map_err(|error| format!("failed to encode relation candidates: {error}"))?;
        connection
            .execute(
                "UPDATE symbol_relations
                 SET target_key = ?2, confidence = ?3, ambiguous = ?4,
                     candidate_target_keys_json = ?5
                 WHERE id = ?1",
                params![
                    id,
                    target_key,
                    confidence,
                    i64::from(ambiguous),
                    candidates_json
                ],
            )
            .map_err(sql_error)?;
    }

    connection
        .execute(
            "DELETE FROM module_relations WHERE project_id = ?1",
            params![project_id],
        )
        .map_err(sql_error)?;
    let includes = {
        let mut statement = connection
            .prepare(
                "SELECT file, resolved_path
                 FROM includes
                 WHERE project_id = ?1 AND resolved = 1 AND resolved_path != ''
                 ORDER BY file, resolved_path, line",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![project_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(sql_error)?;
        let mut values = Vec::new();
        for row in rows {
            values.push(row.map_err(sql_error)?);
        }
        values
    };
    for (source_file, target_file) in includes {
        aggregate_module_relation(
            connection,
            project_id,
            module_for_file(&source_file).as_str(),
            module_for_file(&target_file).as_str(),
            "include",
            "resolved",
        )?;
    }

    let resolved_relations = {
        let mut statement = connection
            .prepare(
                "SELECT source_key, target_key, relation_kind, confidence
                 FROM symbol_relations
                 WHERE project_id = ?1 AND source_key != '' AND target_key != ''
                 ORDER BY id",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![project_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(sql_error)?;
        let mut values = Vec::new();
        for row in rows {
            values.push(row.map_err(sql_error)?);
        }
        values
    };
    for (source_key, target_key, relation_kind, confidence) in resolved_relations {
        let (Some(source), Some(target)) = (
            by_key.get(source_key.as_str()).copied(),
            by_key.get(target_key.as_str()).copied(),
        ) else {
            continue;
        };
        aggregate_module_relation(
            connection,
            project_id,
            module_for_file(&source.file).as_str(),
            module_for_file(&target.file).as_str(),
            relation_kind.as_str(),
            confidence.as_str(),
        )?;
    }

    summary.module_relations = connection
        .query_row(
            "SELECT COUNT(*) FROM module_relations WHERE project_id = ?1",
            params![project_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(sql_error)? as usize;
    Ok(summary)
}

pub(crate) fn upgrade_legacy_project_rows(
    connection: &Connection,
    project_id: i64,
) -> Result<(), String> {
    let previous_status = load_index_status(connection, project_id)?;
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

    if previous_status.schema_version < INDEX_SCHEMA_VERSION || !previous_status.complete {
        resolve_graph_facts(connection, project_id)?;
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
