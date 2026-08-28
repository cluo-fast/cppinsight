mod commands;
pub mod diagnostics;
pub mod graph;
pub mod index;
pub mod secrets;
pub mod workspace;

pub use index::write_index;

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{Emitter, Manager};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LineStats {
    pub total: u64,
    pub code: u64,
    pub comment: u64,
    pub blank: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScanStats {
    pub source_files: u64,
    pub header_files: u64,
    pub inline_files: u64,
    pub total_bytes: u64,
    pub lines: LineStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncludeInfo {
    pub file: String,
    pub line: u64,
    pub target: String,
    pub kind: String,
    #[serde(default)]
    pub resolved: bool,
    #[serde(default)]
    pub resolved_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolInfo {
    pub file: String,
    pub line: u64,
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub qualified_name: String,
    #[serde(default)]
    pub stable_key: String,
    #[serde(default)]
    pub parent_qualified_name: String,
    #[serde(default)]
    pub parent_stable_key: String,
    pub signature: String,
    pub scope: String,
    pub is_definition: bool,
    #[serde(default = "default_confidence")]
    pub confidence: String,
}

fn default_confidence() -> String {
    "heuristic".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallInfo {
    pub file: String,
    pub line: u64,
    pub name: String,
    #[serde(default)]
    pub target_qualified_name: String,
    #[serde(default)]
    pub caller_qualified_name: String,
    #[serde(default)]
    pub caller_stable_key: String,
    #[serde(default = "default_confidence")]
    pub confidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolRelationInfo {
    pub file: String,
    pub line: u64,
    pub source_qualified_name: String,
    pub source_stable_key: String,
    pub target_name: String,
    pub kind: String,
    #[serde(default = "default_confidence")]
    pub confidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryPointInfo {
    pub file: String,
    pub line: u64,
    pub qualified_name: String,
    pub symbol_stable_key: String,
    pub kind: String,
    #[serde(default = "default_confidence")]
    pub confidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: String,
    pub extension: String,
    pub category: String,
    pub size_bytes: u64,
    #[serde(default)]
    pub mtime_unix: u64,
    #[serde(default)]
    pub from_compile_commands: bool,
    #[serde(default)]
    pub reused_from_cache: bool,
    pub lines: LineStats,
    #[serde(default)]
    pub includes: Vec<IncludeInfo>,
    #[serde(default)]
    pub symbols: Vec<SymbolInfo>,
    #[serde(default)]
    pub calls: Vec<CallInfo>,
    #[serde(default)]
    pub relations: Vec<SymbolRelationInfo>,
    #[serde(default)]
    pub entry_points: Vec<EntryPointInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub generator_version: String,
    pub root: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub confidence: String,
    #[serde(default)]
    pub scan_duration_ms: u64,
    #[serde(default)]
    pub compile_commands: String,
    #[serde(default)]
    pub compile_commands_entries: u64,
    #[serde(default)]
    pub reused_file_count: u64,
    pub file_count: usize,
    pub skipped_entries: u64,
    pub stats: ScanStats,
    #[serde(default)]
    pub extensions: BTreeMap<String, u64>,
    #[serde(default)]
    pub top_level_layout: BTreeMap<String, u64>,
    #[serde(default)]
    pub second_level_layout: BTreeMap<String, u64>,
    #[serde(default)]
    pub diagnostics: Vec<String>,
    #[serde(default)]
    pub includes: Vec<IncludeInfo>,
    #[serde(default)]
    pub symbols: Vec<SymbolInfo>,
    #[serde(default)]
    pub calls: Vec<CallInfo>,
    #[serde(default)]
    pub relations: Vec<SymbolRelationInfo>,
    #[serde(default)]
    pub entry_points: Vec<EntryPointInfo>,
    #[serde(default)]
    pub files: Vec<FileInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HotFile {
    pub path: String,
    pub total_lines: u64,
    pub code_lines: u64,
    pub symbol_count: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectOverview {
    pub project_id: i64,
    pub root: String,
    pub indexed_at: String,
    pub file_count: u64,
    pub symbol_count: u64,
    pub include_count: u64,
    pub mode: String,
    pub confidence: String,
    pub stats: ScanStats,
    pub extensions: BTreeMap<String, u64>,
    pub top_level_layout: BTreeMap<String, u64>,
    pub second_level_layout: BTreeMap<String, u64>,
    pub hotspots: Vec<HotFile>,
    pub symbols: Vec<SymbolInfo>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Citation {
    pub file: String,
    pub line: u64,
    pub title: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AskResponse {
    pub answer: String,
    pub citations: Vec<Citation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolHit {
    pub name: String,
    pub file: String,
    pub line: u64,
    pub kind: String,
    pub signature: String,
    pub is_definition: bool,
    pub confidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactNeighbor {
    pub name: String,
    pub file: String,
    pub line: u64,
    pub via_file: String,
    pub via_line: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryPath {
    pub entry: String,
    pub hops: Vec<SymbolHit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactReport {
    pub symbol: String,
    pub definitions: Vec<SymbolHit>,
    pub callers: Vec<ImpactNeighbor>,
    pub callees: Vec<ImpactNeighbor>,
    pub entry_paths: Vec<EntryPath>,
    pub confidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectSymbolResponse {
    pub query: String,
    pub message: String,
    pub candidates: Vec<SymbolHit>,
    pub report: Option<ImpactReport>,
}

#[derive(Debug, Clone)]
pub struct SearchContext {
    pub file: String,
    pub start_line: u64,
    pub end_line: u64,
    pub text: String,
    pub score: i64,
}

fn read_text_lossy(path: &Path) -> Result<String, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub fn index_scan_json_at(scan_json: &str, db_path: &Path) -> Result<ProjectOverview, String> {
    let json_text = read_text_lossy(Path::new(scan_json))
        .map_err(|error| format!("failed to read scan JSON {}: {error}", scan_json))?;
    let scan: ScanResult = serde_json::from_str(&json_text)
        .map_err(|error| format!("failed to parse scan JSON: {error}"))?;
    let mut connection = open_database_at(db_path)?;
    let project_id = write_index(&mut connection, &scan)?;
    overview_from_scan(project_id, &scan)
}

pub async fn ask_project_at(
    project_id: i64,
    question: &str,
    db_path: &Path,
) -> Result<AskResponse, String> {
    ask_project_at_with_settings(project_id, question, db_path, None).await
}

pub async fn ask_project_at_with_settings(
    project_id: i64,
    question: &str,
    db_path: &Path,
    settings_db_path: Option<&Path>,
) -> Result<AskResponse, String> {
    let question_owned = question.to_string();
    let db_path = db_path.to_path_buf();
    let settings_db_path = settings_db_path.map(|path| path.to_path_buf());

    // Keep SQLite / graph retrieval off the UI/async runtime thread.
    let (contexts, ai_settings) = tauri::async_runtime::spawn_blocking(move || {
        let connection = open_database_at(&db_path)?;
        let contexts = search_contexts(&connection, project_id, &question_owned, 8)?;
        let ai_settings = if let Some(settings_path) = settings_db_path.as_ref() {
            let settings = open_database_at(settings_path)?;
            read_openai_settings(&settings)?
        } else {
            read_openai_settings(&connection)?
        };
        Ok::<_, String>((contexts, ai_settings))
    })
    .await
    .map_err(|error| format!("ask retrieval join error: {error}"))??;

    if contexts.is_empty() {
        return Ok(AskResponse {
            answer: "I could not find indexed project context for that question. Re-index the project and try again.".to_string(),
            citations: Vec::new(),
        });
    }

    if ai_settings.api_key.trim().is_empty() {
        return Ok(AskResponse {
            answer: "OpenAI API key is not configured. Set it in Settings, or via OPENAI_API_KEY."
                .to_string(),
            citations: contexts_to_citations(&contexts),
        });
    }

    let answer = call_openai(&ai_settings, question, &contexts).await?;
    Ok(AskResponse {
        answer,
        citations: contexts_to_citations(&contexts),
    })
}

pub fn default_db_path() -> PathBuf {
    if let Ok(path) = std::env::var("CPPINSIGHT_DB") {
        return PathBuf::from(path);
    }

    if let Some(home) = dirs_next_home() {
        return home.join(".cppinsight").join("projects.sqlite");
    }

    PathBuf::from("projects.sqlite")
}

pub fn project_index_db_path(root: &Path) -> PathBuf {
    root.join(".cppinsight").join("index.sqlite")
}

fn dirs_next_home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

pub(crate) fn app_data_dir_from_app(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve app data dir: {error}"))
}

/// Legacy global projects DB (pre project-local index).
pub(crate) fn legacy_projects_db_path_from_app(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir_from_app(app)?.join("projects.sqlite"))
}

pub(crate) fn settings_db_path_from_app(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir_from_app(app)?.join("settings.sqlite"))
}

/// Index DB for a project root. `CPPINSIGHT_DB` overrides (migration skipped by callers).
pub(crate) fn index_db_path_for_root(
    app: &tauri::AppHandle,
    root: &Path,
) -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("CPPINSIGHT_DB") {
        return Ok(PathBuf::from(path));
    }
    let _ = app;
    Ok(project_index_db_path(root))
}

#[cfg(windows)]
pub fn normalize_root_key(root: &str) -> String {
    let path = PathBuf::from(root);
    let canonical = fs::canonicalize(&path).unwrap_or(path);
    let mut value = canonical.to_string_lossy().to_string();
    if let Some(stripped) = value.strip_prefix(r"\\?\") {
        value = stripped.to_string();
    }
    value.replace('/', "\\").to_lowercase()
}

#[cfg(not(windows))]
pub fn normalize_root_key(root: &str) -> String {
    let path = PathBuf::from(root);
    let canonical = fs::canonicalize(&path).unwrap_or(path);
    canonical.to_string_lossy().replace('\\', "/")
}

fn legacy_db_candidates(app: &tauri::AppHandle) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(path) = legacy_projects_db_path_from_app(app) {
        candidates.push(path);
    }
    if let Some(home) = dirs_next_home() {
        candidates.push(home.join(".cppinsight").join("projects.sqlite"));
    }
    candidates
}

fn project_exists_for_root(connection: &Connection, root: &str) -> Result<bool, String> {
    let target = normalize_root_key(root);
    let mut statement = connection
        .prepare("SELECT root FROM projects")
        .map_err(sql_error)?;
    let roots = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(sql_error)?;
    for item in roots {
        let value = item.map_err(sql_error)?;
        if normalize_root_key(&value) == target {
            return Ok(true);
        }
    }
    Ok(false)
}

fn find_legacy_project_id(connection: &Connection, root: &str) -> Result<Option<i64>, String> {
    let target = normalize_root_key(root);
    let mut statement = connection
        .prepare("SELECT id, root FROM projects")
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(sql_error)?;
    for row in rows {
        let (id, stored_root) = row.map_err(sql_error)?;
        if normalize_root_key(&stored_root) == target {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

fn copy_project_between_dbs(
    source: &Connection,
    destination: &Connection,
    source_project_id: i64,
) -> Result<i64, String> {
    let (
        root,
        indexed_at,
        file_count,
        symbol_count,
        include_count,
        diagnostics_json,
        mode,
        confidence,
        index_schema_version,
        generator_version,
        index_generation,
        index_complete,
    ): (
        String,
        String,
        i64,
        i64,
        i64,
        String,
        String,
        String,
        i64,
        String,
        i64,
        i64,
    ) = source
        .query_row(
            "SELECT root, indexed_at, file_count, symbol_count, include_count,
                    diagnostics_json, mode, confidence, index_schema_version,
                    generator_version, index_generation, index_complete
             FROM projects WHERE id = ?1",
            params![source_project_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                    row.get(11)?,
                ))
            },
        )
        .map_err(sql_error)?;

    destination
        .execute(
            "INSERT INTO projects(
                root, indexed_at, file_count, symbol_count, include_count,
                diagnostics_json, mode, confidence, index_schema_version,
                generator_version, index_generation, index_complete
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(root) DO UPDATE SET
                indexed_at = excluded.indexed_at,
                file_count = excluded.file_count,
                symbol_count = excluded.symbol_count,
                include_count = excluded.include_count,
                diagnostics_json = excluded.diagnostics_json,
                mode = excluded.mode,
                confidence = excluded.confidence,
                index_schema_version = excluded.index_schema_version,
                generator_version = excluded.generator_version,
                index_generation = excluded.index_generation,
                index_complete = excluded.index_complete",
            params![
                root,
                indexed_at,
                file_count,
                symbol_count,
                include_count,
                diagnostics_json,
                mode,
                confidence,
                index_schema_version,
                generator_version,
                index_generation,
                index_complete,
            ],
        )
        .map_err(sql_error)?;

    let dest_project_id: i64 = destination
        .query_row(
            "SELECT id FROM projects WHERE root = ?1",
            params![root],
            |row| row.get(0),
        )
        .map_err(sql_error)?;

    destination
        .execute(
            "DELETE FROM files WHERE project_id = ?1",
            params![dest_project_id],
        )
        .map_err(sql_error)?;
    destination
        .execute(
            "DELETE FROM symbols WHERE project_id = ?1",
            params![dest_project_id],
        )
        .map_err(sql_error)?;
    destination
        .execute(
            "DELETE FROM includes WHERE project_id = ?1",
            params![dest_project_id],
        )
        .map_err(sql_error)?;
    destination
        .execute(
            "DELETE FROM calls WHERE project_id = ?1",
            params![dest_project_id],
        )
        .map_err(sql_error)?;
    destination
        .execute(
            "DELETE FROM chunks WHERE project_id = ?1",
            params![dest_project_id],
        )
        .map_err(sql_error)?;
    for table in [
        "symbol_relations",
        "module_relations",
        "entry_points",
        "graph_layouts",
    ] {
        destination
            .execute(
                &format!("DELETE FROM {table} WHERE project_id = ?1"),
                params![dest_project_id],
            )
            .map_err(sql_error)?;
    }

    {
        let mut statement = source
            .prepare(
                "SELECT path, extension, category, size_bytes, total_lines, code_lines, comment_lines, blank_lines, mtime_unix
                 FROM files WHERE project_id = ?1",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![source_project_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                ))
            })
            .map_err(sql_error)?;
        for row in rows {
            let (
                path,
                extension,
                category,
                size_bytes,
                total_lines,
                code_lines,
                comment_lines,
                blank_lines,
                mtime_unix,
            ) = row.map_err(sql_error)?;
            destination
                .execute(
                    "INSERT INTO files(project_id, path, extension, category, size_bytes, total_lines, code_lines, comment_lines, blank_lines, mtime_unix)
                     VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        dest_project_id,
                        path,
                        extension,
                        category,
                        size_bytes,
                        total_lines,
                        code_lines,
                        comment_lines,
                        blank_lines,
                        mtime_unix
                    ],
                )
                .map_err(sql_error)?;
        }
    }

    {
        let mut statement = source
            .prepare(
                "SELECT file, line, kind, name, qualified_name, stable_key,
                        parent_qualified_name, parent_stable_key, signature, scope,
                        is_definition, confidence
                 FROM symbols WHERE project_id = ?1",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![source_project_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, String>(11)?,
                ))
            })
            .map_err(sql_error)?;
        for row in rows {
            let (
                file,
                line,
                kind,
                name,
                qualified_name,
                stable_key,
                parent_qualified_name,
                parent_stable_key,
                signature,
                scope,
                is_definition,
                confidence,
            ) = row.map_err(sql_error)?;
            destination
                .execute(
                    "INSERT INTO symbols(
                        project_id, file, line, kind, name, qualified_name, stable_key,
                        parent_qualified_name, parent_stable_key, signature, scope,
                        is_definition, confidence
                     ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    params![
                        dest_project_id,
                        file,
                        line,
                        kind,
                        name,
                        qualified_name,
                        stable_key,
                        parent_qualified_name,
                        parent_stable_key,
                        signature,
                        scope,
                        is_definition,
                        confidence
                    ],
                )
                .map_err(sql_error)?;
        }
    }

    {
        let mut statement = source
            .prepare(
                "SELECT file, line, target, kind, resolved, resolved_path
                 FROM includes WHERE project_id = ?1",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![source_project_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(sql_error)?;
        for row in rows {
            let (file, line, target, kind, resolved, resolved_path) = row.map_err(sql_error)?;
            destination
                .execute(
                    "INSERT INTO includes(project_id, file, line, target, kind, resolved, resolved_path)
                     VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![dest_project_id, file, line, target, kind, resolved, resolved_path],
                )
                .map_err(sql_error)?;
        }
    }

    if table_exists(source, "calls")? {
        let mut statement = source
            .prepare(
                "SELECT file, line, name, target_qualified_name, caller_qualified_name,
                        caller_stable_key, resolved_target_key, candidate_target_keys_json,
                        confidence
                 FROM calls WHERE project_id = ?1",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![source_project_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                ))
            })
            .map_err(sql_error)?;
        for row in rows {
            let (
                file,
                line,
                name,
                target_qualified_name,
                caller_qualified_name,
                caller_stable_key,
                resolved_target_key,
                candidate_target_keys_json,
                confidence,
            ) = row.map_err(sql_error)?;
            destination
                .execute(
                    "INSERT INTO calls(
                        project_id, file, line, name, target_qualified_name,
                        caller_qualified_name, caller_stable_key, resolved_target_key,
                        candidate_target_keys_json, confidence
                     ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        dest_project_id,
                        file,
                        line,
                        name,
                        target_qualified_name,
                        caller_qualified_name,
                        caller_stable_key,
                        resolved_target_key,
                        candidate_target_keys_json,
                        confidence
                    ],
                )
                .map_err(sql_error)?;
        }
    }

    if table_exists(source, "symbol_relations")? {
        let mut statement = source
            .prepare(
                "SELECT source_key, target_key, target_name, relation_kind,
                        evidence_file, evidence_line, confidence, ambiguous,
                        candidate_target_keys_json
                 FROM symbol_relations WHERE project_id = ?1 ORDER BY id",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![source_project_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, String>(8)?,
                ))
            })
            .map_err(sql_error)?;
        for row in rows {
            let (
                source_key,
                target_key,
                target_name,
                relation_kind,
                evidence_file,
                evidence_line,
                confidence,
                ambiguous,
                candidate_target_keys_json,
            ) = row.map_err(sql_error)?;
            destination
                .execute(
                    "INSERT INTO symbol_relations(
                        project_id, source_key, target_key, target_name, relation_kind,
                        evidence_file, evidence_line, confidence, ambiguous,
                        candidate_target_keys_json
                     ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        dest_project_id,
                        source_key,
                        target_key,
                        target_name,
                        relation_kind,
                        evidence_file,
                        evidence_line,
                        confidence,
                        ambiguous,
                        candidate_target_keys_json
                    ],
                )
                .map_err(sql_error)?;
        }
    }

    if table_exists(source, "module_relations")? {
        let mut statement = source
            .prepare(
                "SELECT source_module, target_module, relation_kind, weight,
                        confidence, evidence_count
                 FROM module_relations WHERE project_id = ?1
                 ORDER BY source_module, target_module, relation_kind",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![source_project_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })
            .map_err(sql_error)?;
        for row in rows {
            let (source_module, target_module, relation_kind, weight, confidence, evidence_count) =
                row.map_err(sql_error)?;
            destination
                .execute(
                    "INSERT INTO module_relations(
                        project_id, source_module, target_module, relation_kind,
                        weight, confidence, evidence_count
                     ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        dest_project_id,
                        source_module,
                        target_module,
                        relation_kind,
                        weight,
                        confidence,
                        evidence_count
                    ],
                )
                .map_err(sql_error)?;
        }
    }

    if table_exists(source, "entry_points")? {
        let mut statement = source
            .prepare(
                "SELECT symbol_key, qualified_name, entry_kind, evidence_file,
                        evidence_line, confidence
                 FROM entry_points WHERE project_id = ?1 ORDER BY id",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![source_project_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(sql_error)?;
        for row in rows {
            let (symbol_key, qualified_name, entry_kind, evidence_file, evidence_line, confidence) =
                row.map_err(sql_error)?;
            destination
                .execute(
                    "INSERT INTO entry_points(
                        project_id, symbol_key, qualified_name, entry_kind,
                        evidence_file, evidence_line, confidence
                     ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        dest_project_id,
                        symbol_key,
                        qualified_name,
                        entry_kind,
                        evidence_file,
                        evidence_line,
                        confidence
                    ],
                )
                .map_err(sql_error)?;
        }
    }

    if table_exists(source, "graph_layouts")? {
        let mut statement = source
            .prepare(
                "SELECT graph_kind, view_key, protocol_version, positions_json, updated_at
                 FROM graph_layouts WHERE project_id = ?1 ORDER BY graph_kind, view_key",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![source_project_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(sql_error)?;
        for row in rows {
            let (graph_kind, view_key, protocol_version, positions_json, updated_at) =
                row.map_err(sql_error)?;
            destination
                .execute(
                    "INSERT INTO graph_layouts(
                        project_id, graph_kind, view_key, protocol_version,
                        positions_json, updated_at
                     ) VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        dest_project_id,
                        graph_kind,
                        view_key,
                        protocol_version,
                        positions_json,
                        updated_at
                    ],
                )
                .map_err(sql_error)?;
        }
    }

    {
        let mut statement = source
            .prepare("SELECT file, start_line, end_line, text FROM chunks WHERE project_id = ?1")
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![source_project_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(sql_error)?;
        for row in rows {
            let (file, start_line, end_line, text) = row.map_err(sql_error)?;
            destination
                .execute(
                    "INSERT INTO chunks(project_id, file, start_line, end_line, text)
                     VALUES(?1, ?2, ?3, ?4, ?5)",
                    params![dest_project_id, file, start_line, end_line, text],
                )
                .map_err(sql_error)?;
        }
    }

    index::upgrade_legacy_project_rows(destination, dest_project_id)?;
    Ok(dest_project_id)
}

fn settings_table_api_key(connection: &Connection) -> Result<Option<String>, String> {
    read_setting_value(connection, "openai_api_key")
}

fn migrate_api_key_once(legacy: &Connection, settings_db: &Path) -> Result<(), String> {
    let settings = open_database_at(settings_db)?;
    migrate_api_key_once_with_backend(legacy, &settings, &secrets::OsSecretBackend)
}

fn migrate_api_key_once_with_backend(
    legacy: &Connection,
    settings: &Connection,
    backend: &dyn secrets::SecretBackend,
) -> Result<(), String> {
    secrets::migrate_legacy_api_key(settings, backend)?;
    if backend.get(secrets::OPENAI_ACCOUNT)?.is_none() {
        if let Some(secret) = settings_table_api_key(legacy)? {
            backend.set(secrets::OPENAI_ACCOUNT, &secret)?;
        }
    }
    legacy
        .execute("DELETE FROM settings WHERE key = 'openai_api_key'", [])
        .map_err(sql_error)?;
    Ok(())
}

fn migrate_legacy_api_keys_from_paths(
    settings: &Connection,
    candidates: &[PathBuf],
    backend: &dyn secrets::SecretBackend,
) -> Result<(), String> {
    secrets::migrate_legacy_api_key(settings, backend)?;
    for path in candidates {
        if !path.exists() {
            continue;
        }
        let legacy = Connection::open(path)
            .map_err(|error| format!("failed to open legacy settings database: {error}"))?;
        migrate_database(&legacy)?;
        migrate_api_key_once_with_backend(&legacy, settings, backend)?;
    }
    Ok(())
}

fn deduplicate_legacy_candidates(candidates: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .filter(|path| {
            let key = if cfg!(windows) {
                path.to_string_lossy().to_ascii_lowercase()
            } else {
                path.to_string_lossy().to_string()
            };
            seen.insert(key)
        })
        .collect()
}

pub(crate) fn migrate_legacy_api_keys(app: &tauri::AppHandle) -> Result<(), String> {
    let settings_path = settings_db_path_from_app(app)?;
    let settings = open_database_at(&settings_path)?;
    let candidates = deduplicate_legacy_candidates(legacy_db_candidates(app));
    migrate_legacy_api_keys_from_paths(&settings, &candidates, &secrets::OsSecretBackend)
}

/// Migrate one project from legacy global DB into project-local DB. Best-effort.
pub(crate) fn migrate_legacy_project_if_needed(
    app: &tauri::AppHandle,
    root: &Path,
    local_db_path: &Path,
) -> Result<(), String> {
    if std::env::var("CPPINSIGHT_DB").is_ok() {
        return Ok(());
    }

    let root_str = root.to_string_lossy().to_string();
    let local = open_database_at(local_db_path)?;
    let already_local = project_exists_for_root(&local, &root_str)?;
    let settings_path = settings_db_path_from_app(app)?;

    for legacy_path in legacy_db_candidates(app) {
        if !legacy_path.exists() {
            continue;
        }
        let legacy = match Connection::open(&legacy_path) {
            Ok(connection) => connection,
            Err(_) => continue,
        };
        let _ = legacy.execute_batch("PRAGMA foreign_keys = ON;");
        if migrate_database(&legacy).is_err() {
            continue;
        }

        if let Err(error) = migrate_api_key_once(&legacy, &settings_path) {
            eprintln!("warning: legacy API key migration deferred: {error}");
        }

        let Some(source_id) = find_legacy_project_id(&legacy, &root_str)? else {
            continue;
        };

        if !already_local {
            copy_project_between_dbs(&legacy, &local, source_id)?;
        }

        legacy
            .execute("DELETE FROM projects WHERE id = ?1", params![source_id])
            .map_err(sql_error)?;
        return Ok(());
    }

    Ok(())
}

pub fn open_database_at(db_path: &Path) -> Result<Connection, String> {
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create db dir {}: {error}", parent.display()))?;
    }

    let connection = Connection::open(db_path).map_err(sql_error)?;
    migrate_database(&connection)?;
    Ok(connection)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedScanProgress {
    pub files_discovered: u64,
    pub files_analyzed: u64,
    pub files_reused: u64,
    pub current_file: String,
}

pub fn parse_scan_progress_line(line: &str) -> Option<ParsedScanProgress> {
    let line = line.trim();
    if !line.starts_with("[scan]") {
        return None;
    }

    let mut files_discovered = None;
    let mut files_analyzed = None;
    let mut files_reused = None;
    let mut current_file = None;

    for token in line.split_whitespace().skip(1) {
        if let Some(value) = token.strip_prefix("files=") {
            files_discovered = value.parse().ok();
        } else if let Some(value) = token.strip_prefix("analyzed=") {
            files_analyzed = value.parse().ok();
        } else if let Some(value) = token.strip_prefix("reused=") {
            files_reused = value.parse().ok();
        } else if let Some(value) = token.strip_prefix("current=") {
            current_file = Some(value.to_string());
        }
    }

    Some(ParsedScanProgress {
        files_discovered: files_discovered?,
        files_analyzed: files_analyzed?,
        files_reused: files_reused?,
        current_file: current_file?,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexProgressEvent {
    pub stage: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_discovered: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_analyzed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_reused: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

pub(crate) fn emit_index_progress(app: &tauri::AppHandle, event: &IndexProgressEvent) {
    let _ = app.emit("index-progress", event);
}

pub fn run_scan(path: &str) -> Result<ScanResult, String> {
    run_scan_with_progress(path, |_| {})
}

fn summarize_cli_stderr(stderr_lines: &[String]) -> String {
    let mut errors: Vec<&str> = stderr_lines
        .iter()
        .map(|line| line.trim())
        .filter(|line| {
            !line.is_empty()
                && (line.starts_with("[ERROR]")
                    || line.contains("error:")
                    || line.contains("Error:")
                    || line.contains("failed")
                    || line.contains("Failed"))
        })
        .collect();
    if errors.is_empty() {
        errors = stderr_lines
            .iter()
            .rev()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty() && !line.starts_with("[scan]"))
            .take(5)
            .collect();
        errors.reverse();
    }
    if errors.is_empty() {
        return "no stderr details".to_string();
    }
    errors.into_iter().take(6).collect::<Vec<_>>().join(" | ")
}

pub fn run_scan_with_progress(
    path: &str,
    mut on_progress: impl FnMut(IndexProgressEvent),
) -> Result<ScanResult, String> {
    let cli = find_cppinsight_binary()?;
    let stamp = format!("{}-{}", std::process::id(), Utc::now().timestamp_millis());
    let json_path = std::env::temp_dir().join(format!("cppinsight-scan-{stamp}.json"));
    // Keep overview out of the project tree so a read-only/locked project root
    // does not fail the desktop index path after a successful scan.
    let overview_path = std::env::temp_dir().join(format!("cppinsight-overview-{stamp}.md"));

    let mut child = Command::new(&cli)
        .arg("scan")
        .arg(path)
        .arg("--json")
        .arg(&json_path)
        .arg("--overview")
        .arg(&overview_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to run cppinsight CLI ({}): {error}", cli.display()))?;

    let mut stderr_lines = Vec::new();
    if let Some(stderr) = child.stderr.take() {
        let mut reader = BufReader::new(stderr);
        loop {
            let mut raw = Vec::new();
            let read = reader
                .read_until(b'\n', &mut raw)
                .map_err(|error| format!("failed to read cppinsight stderr: {error}"))?;
            if read == 0 {
                break;
            }
            let line = String::from_utf8_lossy(&raw)
                .trim_end_matches(['\r', '\n'])
                .to_string();
            if line.is_empty() {
                continue;
            }
            if let Some(parsed) = parse_scan_progress_line(&line) {
                on_progress(IndexProgressEvent {
                    stage: "scanning".to_string(),
                    files_discovered: Some(parsed.files_discovered),
                    files_analyzed: Some(parsed.files_analyzed),
                    files_reused: Some(parsed.files_reused),
                    current_file: Some(parsed.current_file),
                    message: None,
                });
            }
            stderr_lines.push(line);
        }
    }

    let status = child
        .wait()
        .map_err(|error| format!("failed to wait for cppinsight CLI: {error}"))?;

    if !status.success() {
        let _ = fs::remove_file(&json_path);
        let _ = fs::remove_file(&overview_path);
        return Err(format!(
            "cppinsight scan failed with status {status}. cli={} path={} detail={}",
            cli.display(),
            path,
            summarize_cli_stderr(&stderr_lines)
        ));
    }

    let json_text = match read_text_lossy(&json_path) {
        Ok(text) => text,
        Err(error) => {
            let _ = fs::remove_file(&json_path);
            let _ = fs::remove_file(&overview_path);
            return Err(format!(
                "failed to read scan JSON {}: {}. detail={}",
                json_path.display(),
                error,
                summarize_cli_stderr(&stderr_lines)
            ));
        }
    };
    let _ = fs::remove_file(&json_path);
    let _ = fs::remove_file(&overview_path);

    serde_json::from_str(&json_text).map_err(|error| format!("failed to parse scan JSON: {error}"))
}

pub fn find_cppinsight_binary() -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("CPPINSIGHT_BIN") {
        let candidate = PathBuf::from(path);
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    let mut candidates = Vec::new();
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            candidates.push(parent.join("cppinsight.exe"));
            candidates.push(parent.join("cppinsight"));
            candidates.push(parent.join("resources").join("cppinsight.exe"));
            candidates.push(parent.join("..").join("resources").join("cppinsight.exe"));
            candidates.push(parent.join("..").join("cppinsight.exe"));
            // Dev layout: desktop/src-tauri/target/... -> repo/build
            candidates.push(parent.join("../../../resources/cppinsight.exe"));
            candidates.push(parent.join("../../../../build/cppinsight.exe"));
            candidates.push(parent.join("../../../../build/Release/cppinsight.exe"));
            candidates.push(
                parent
                    .join("..")
                    .join("..")
                    .join("build")
                    .join("cppinsight.exe"),
            );
            candidates.push(
                parent
                    .join("..")
                    .join("..")
                    .join("build")
                    .join("Release")
                    .join("cppinsight.exe"),
            );
            candidates.push(
                parent
                    .join("..")
                    .join("..")
                    .join("build-codex")
                    .join("Release")
                    .join("cppinsight.exe"),
            );
        }
    }

    candidates.push(PathBuf::from("resources/cppinsight.exe"));
    candidates.push(PathBuf::from("../build/cppinsight.exe"));
    candidates.push(PathBuf::from("../build/Release/cppinsight.exe"));
    candidates.push(PathBuf::from("../build-codex/Release/cppinsight.exe"));
    candidates.push(PathBuf::from("../build-codex/Debug/cppinsight.exe"));
    candidates.push(PathBuf::from("../build/Debug/cppinsight.exe"));

    for candidate in candidates {
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    Err("未找到扫描引擎 cppinsight.exe。请先运行仓库根目录的「启动CppInsight.bat」，或设置环境变量 CPPINSIGHT_BIN。".to_string())
}

fn migrate_database(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS projects (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                root TEXT NOT NULL UNIQUE,
                indexed_at TEXT NOT NULL,
                file_count INTEGER NOT NULL,
                symbol_count INTEGER NOT NULL,
                include_count INTEGER NOT NULL,
                diagnostics_json TEXT NOT NULL,
                mode TEXT NOT NULL DEFAULT 'heuristic',
                confidence TEXT NOT NULL DEFAULT 'heuristic'
            );

            CREATE TABLE IF NOT EXISTS files (
                project_id INTEGER NOT NULL,
                path TEXT NOT NULL,
                extension TEXT NOT NULL,
                category TEXT NOT NULL,
                size_bytes INTEGER NOT NULL,
                total_lines INTEGER NOT NULL,
                code_lines INTEGER NOT NULL,
                comment_lines INTEGER NOT NULL,
                blank_lines INTEGER NOT NULL,
                mtime_unix INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY(project_id, path),
                FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS symbols (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_id INTEGER NOT NULL,
                file TEXT NOT NULL,
                line INTEGER NOT NULL,
                kind TEXT NOT NULL,
                name TEXT NOT NULL,
                signature TEXT NOT NULL,
                scope TEXT NOT NULL,
                is_definition INTEGER NOT NULL,
                confidence TEXT NOT NULL DEFAULT 'heuristic',
                FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS includes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_id INTEGER NOT NULL,
                file TEXT NOT NULL,
                line INTEGER NOT NULL,
                target TEXT NOT NULL,
                kind TEXT NOT NULL,
                resolved INTEGER NOT NULL DEFAULT 0,
                resolved_path TEXT NOT NULL DEFAULT '',
                FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS calls (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_id INTEGER NOT NULL,
                file TEXT NOT NULL,
                line INTEGER NOT NULL,
                name TEXT NOT NULL,
                confidence TEXT NOT NULL DEFAULT 'heuristic',
                FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS chunks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_id INTEGER NOT NULL,
                file TEXT NOT NULL,
                start_line INTEGER NOT NULL,
                end_line INTEGER NOT NULL,
                text TEXT NOT NULL,
                FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_symbols_project_name ON symbols(project_id, name);
            CREATE INDEX IF NOT EXISTS idx_calls_project_name ON calls(project_id, name);
            ",
        )
        .map_err(sql_error)?;

    index::migrate_legacy_base_schema(connection)?;
    index::migrate_graph_schema(connection)
}

pub(crate) fn overview_from_scan(
    project_id: i64,
    scan: &ScanResult,
) -> Result<ProjectOverview, String> {
    let mut hotspots = scan
        .files
        .iter()
        .map(|file| HotFile {
            path: file.path.clone(),
            total_lines: file.lines.total,
            code_lines: file.lines.code,
            symbol_count: scan
                .symbols
                .iter()
                .filter(|symbol| symbol.file == file.path)
                .count() as u64,
        })
        .collect::<Vec<_>>();
    hotspots.sort_by_key(|item| std::cmp::Reverse(item.code_lines));
    hotspots.truncate(8);

    let mut symbols = scan.symbols.clone();
    symbols.sort_by(|left, right| left.file.cmp(&right.file).then(left.line.cmp(&right.line)));
    symbols.truncate(200);

    Ok(ProjectOverview {
        project_id,
        root: scan.root.clone(),
        indexed_at: Utc::now().to_rfc3339(),
        file_count: scan.file_count as u64,
        symbol_count: scan.symbols.len() as u64,
        include_count: scan.includes.len() as u64,
        mode: if scan.mode.is_empty() {
            "heuristic".to_string()
        } else {
            scan.mode.clone()
        },
        confidence: if scan.confidence.is_empty() {
            "heuristic".to_string()
        } else {
            scan.confidence.clone()
        },
        stats: scan.stats.clone(),
        extensions: scan.extensions.clone(),
        top_level_layout: scan.top_level_layout.clone(),
        second_level_layout: scan.second_level_layout.clone(),
        hotspots,
        symbols,
        diagnostics: scan.diagnostics.clone(),
    })
}

pub fn load_project_overview(
    connection: &Connection,
    project_id: i64,
) -> Result<ProjectOverview, String> {
    let (
        root,
        indexed_at,
        file_count,
        symbol_count,
        include_count,
        diagnostics_json,
        mode,
        confidence,
    ): (String, String, u64, u64, u64, String, String, String) = connection
        .query_row(
            "SELECT root, indexed_at, file_count, symbol_count, include_count, diagnostics_json,
                    COALESCE(mode, 'heuristic'), COALESCE(confidence, 'heuristic')
             FROM projects WHERE id = ?1",
            params![project_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get::<_, i64>(2)? as u64,
                    row.get::<_, i64>(3)? as u64,
                    row.get::<_, i64>(4)? as u64,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            },
        )
        .map_err(sql_error)?;

    let diagnostics = serde_json::from_str(&diagnostics_json).unwrap_or_default();
    let stats = load_stats(connection, project_id)?;
    let extensions = load_group_counts(connection, project_id, "extension")?;
    let top_level_layout = load_top_level_layout(connection, project_id)?;
    let second_level_layout = load_second_level_layout(connection, project_id)?;
    let hotspots = load_hotspots(connection, project_id)?;
    let symbols = load_symbols(connection, project_id, 200)?;

    Ok(ProjectOverview {
        project_id,
        root,
        indexed_at,
        file_count,
        symbol_count,
        include_count,
        mode,
        confidence,
        stats,
        extensions,
        top_level_layout,
        second_level_layout,
        hotspots,
        symbols,
        diagnostics,
    })
}

fn load_stats(connection: &Connection, project_id: i64) -> Result<ScanStats, String> {
    connection
        .query_row(
            "
            SELECT
                COALESCE(SUM(CASE WHEN category = 'source' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN category = 'header' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN category = 'inline' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(size_bytes), 0),
                COALESCE(SUM(total_lines), 0),
                COALESCE(SUM(code_lines), 0),
                COALESCE(SUM(comment_lines), 0),
                COALESCE(SUM(blank_lines), 0)
             FROM files WHERE project_id = ?1",
            params![project_id],
            |row| {
                Ok(ScanStats {
                    source_files: row.get::<_, i64>(0)? as u64,
                    header_files: row.get::<_, i64>(1)? as u64,
                    inline_files: row.get::<_, i64>(2)? as u64,
                    total_bytes: row.get::<_, i64>(3)? as u64,
                    lines: LineStats {
                        total: row.get::<_, i64>(4)? as u64,
                        code: row.get::<_, i64>(5)? as u64,
                        comment: row.get::<_, i64>(6)? as u64,
                        blank: row.get::<_, i64>(7)? as u64,
                    },
                })
            },
        )
        .map_err(sql_error)
}

fn load_group_counts(
    connection: &Connection,
    project_id: i64,
    column_name: &str,
) -> Result<BTreeMap<String, u64>, String> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT {column_name}, COUNT(*) FROM files WHERE project_id = ?1 GROUP BY {column_name} ORDER BY {column_name}"
        ))
        .map_err(sql_error)?;

    let rows = statement
        .query_map(params![project_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
        })
        .map_err(sql_error)?;

    let mut counts = BTreeMap::new();
    for row in rows {
        let (key, value) = row.map_err(sql_error)?;
        counts.insert(key, value);
    }
    Ok(counts)
}

fn load_top_level_layout(
    connection: &Connection,
    project_id: i64,
) -> Result<BTreeMap<String, u64>, String> {
    let mut statement = connection
        .prepare("SELECT path FROM files WHERE project_id = ?1")
        .map_err(sql_error)?;
    let rows = statement
        .query_map(params![project_id], |row| row.get::<_, String>(0))
        .map_err(sql_error)?;

    let mut counts = BTreeMap::new();
    for row in rows {
        let path = row.map_err(sql_error)?;
        let top = path.split('/').next().unwrap_or(".").to_string();
        *counts.entry(top).or_insert(0) += 1;
    }
    Ok(counts)
}

fn load_second_level_layout(
    connection: &Connection,
    project_id: i64,
) -> Result<BTreeMap<String, u64>, String> {
    let mut statement = connection
        .prepare("SELECT path FROM files WHERE project_id = ?1")
        .map_err(sql_error)?;
    let rows = statement
        .query_map(params![project_id], |row| row.get::<_, String>(0))
        .map_err(sql_error)?;

    let mut counts = BTreeMap::new();
    for row in rows {
        let path = row.map_err(sql_error)?;
        let mut parts = path.split('/');
        let first = parts.next().unwrap_or(".");
        let second = parts.next();
        let key = match second {
            Some(value) => format!("{first}/{value}"),
            None => first.to_string(),
        };
        *counts.entry(key).or_insert(0) += 1;
    }
    Ok(counts)
}

fn load_hotspots(connection: &Connection, project_id: i64) -> Result<Vec<HotFile>, String> {
    let mut statement = connection
        .prepare(
            "SELECT path, total_lines, code_lines
             FROM files WHERE project_id = ?1
             ORDER BY code_lines DESC, path ASC LIMIT 8",
        )
        .map_err(sql_error)?;

    let rows = statement
        .query_map(params![project_id], |row| {
            Ok(HotFile {
                path: row.get(0)?,
                total_lines: row.get::<_, i64>(1)? as u64,
                code_lines: row.get::<_, i64>(2)? as u64,
                symbol_count: 0,
            })
        })
        .map_err(sql_error)?;

    let mut hotspots = Vec::new();
    for row in rows {
        let mut hot_file = row.map_err(sql_error)?;
        hot_file.symbol_count = connection
            .query_row(
                "SELECT COUNT(*) FROM symbols WHERE project_id = ?1 AND file = ?2",
                params![project_id, hot_file.path],
                |row| row.get::<_, i64>(0),
            )
            .map_err(sql_error)? as u64;
        hotspots.push(hot_file);
    }
    Ok(hotspots)
}

fn load_symbols(
    connection: &Connection,
    project_id: i64,
    limit: i64,
) -> Result<Vec<SymbolInfo>, String> {
    let mut statement = connection
        .prepare(
            "SELECT file, line, kind, name, qualified_name, stable_key,
                    parent_qualified_name, parent_stable_key, signature, scope,
                    is_definition, COALESCE(confidence, 'heuristic')
             FROM symbols WHERE project_id = ?1
             ORDER BY file ASC, line ASC LIMIT ?2",
        )
        .map_err(sql_error)?;

    let rows = statement
        .query_map(params![project_id, limit], |row| {
            Ok(SymbolInfo {
                file: row.get(0)?,
                line: row.get::<_, i64>(1)? as u64,
                kind: row.get(2)?,
                name: row.get(3)?,
                qualified_name: row.get(4)?,
                stable_key: row.get(5)?,
                parent_qualified_name: row.get(6)?,
                parent_stable_key: row.get(7)?,
                signature: row.get(8)?,
                scope: row.get(9)?,
                is_definition: row.get::<_, i64>(10)? != 0,
                confidence: row.get(11)?,
            })
        })
        .map_err(sql_error)?;

    let mut symbols = Vec::new();
    for row in rows {
        symbols.push(row.map_err(sql_error)?);
    }
    Ok(symbols)
}

pub(crate) fn build_chunks(scan: &ScanResult) -> Result<Vec<SearchContext>, String> {
    const MAX_LINES: u64 = 80;
    let root = PathBuf::from(&scan.root);
    let mut chunks = Vec::new();

    for file in &scan.files {
        let path = root.join(Path::new(&file.path));
        let input = match fs::File::open(&path) {
            Ok(file) => file,
            Err(_) => continue,
        };

        let mut current = String::new();
        let mut start_line = 1_u64;
        let mut end_line = 0_u64;
        let mut reader = BufReader::new(input);
        let mut line_number = 0_u64;

        loop {
            let mut raw = Vec::new();
            let read = reader
                .read_until(b'\n', &mut raw)
                .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
            if read == 0 {
                break;
            }
            if raw.last() == Some(&b'\n') {
                raw.pop();
            }
            if raw.last() == Some(&b'\r') {
                raw.pop();
            }

            line_number += 1;
            let line = String::from_utf8_lossy(&raw);
            if current.is_empty() {
                start_line = line_number;
            }
            current.push_str(&line);
            current.push('\n');
            end_line = line_number;

            if line_number - start_line + 1 >= MAX_LINES {
                chunks.push(SearchContext {
                    file: file.path.clone(),
                    start_line,
                    end_line,
                    text: current.clone(),
                    score: 0,
                });
                current.clear();
            }
        }

        if !current.is_empty() {
            chunks.push(SearchContext {
                file: file.path.clone(),
                start_line,
                end_line,
                text: current,
                score: 0,
            });
        }
    }

    Ok(chunks)
}

fn top_level_dir(path: &str) -> String {
    path.split(['/', '\\']).next().unwrap_or(".").to_string()
}

fn is_overview_question(question: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "目录",
        "模块",
        "结构",
        "布局",
        "顶层",
        "负责",
        "overview",
        "layout",
        "module",
        "directory",
        "folder",
        "structure",
    ];
    let lower = question.to_lowercase();
    KEYWORDS.iter().any(|keyword| lower.contains(keyword))
}

fn build_project_layout_context(
    connection: &Connection,
    project_id: i64,
) -> Result<SearchContext, String> {
    let root: String = connection
        .query_row(
            "SELECT root FROM projects WHERE id = ?1",
            params![project_id],
            |row| row.get(0),
        )
        .map_err(sql_error)?;

    let mut statement = connection
        .prepare("SELECT path FROM files WHERE project_id = ?1 ORDER BY path")
        .map_err(sql_error)?;
    let rows = statement
        .query_map(params![project_id], |row| row.get::<_, String>(0))
        .map_err(sql_error)?;

    let mut by_dir: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for row in rows {
        let path = row.map_err(sql_error)?;
        let dir = top_level_dir(&path);
        by_dir.entry(dir).or_default().push(path);
    }

    let mut text = String::new();
    text.push_str(&format!("Project root: {root}\n"));
    text.push_str("Top-level directories and indexed C/C++ files:\n");
    for (dir, paths) in &by_dir {
        text.push_str(&format!("- {dir}/ ({count} files)\n", count = paths.len()));
        for path in paths.iter().take(8) {
            text.push_str(&format!("  - {path}\n"));
        }
        if paths.len() > 8 {
            text.push_str(&format!("  - ... {} more\n", paths.len() - 8));
        }
    }

    Ok(SearchContext {
        file: "<project-layout>".to_string(),
        start_line: 1,
        end_line: text.lines().count() as u64,
        text,
        score: 1_000,
    })
}

fn select_diverse_contexts(mut ranked: Vec<SearchContext>, limit: usize) -> Vec<SearchContext> {
    if limit == 0 {
        return Vec::new();
    }

    ranked.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then(left.file.cmp(&right.file))
            .then(left.start_line.cmp(&right.start_line))
    });

    let mut selected = Vec::new();
    let mut per_dir: BTreeMap<String, usize> = BTreeMap::new();
    let max_per_dir = std::cmp::max(1, limit / 2);

    for context in &ranked {
        if selected.len() >= limit {
            break;
        }
        let dir = top_level_dir(&context.file);
        let count = per_dir.get(&dir).copied().unwrap_or(0);
        if count >= max_per_dir {
            continue;
        }
        *per_dir.entry(dir).or_insert(0) += 1;
        selected.push(context.clone());
    }

    // Fill remaining slots if diversity filter skipped too many.
    if selected.len() < limit {
        for context in ranked {
            if selected.len() >= limit {
                break;
            }
            if selected
                .iter()
                .any(|item| item.file == context.file && item.start_line == context.start_line)
            {
                continue;
            }
            selected.push(context);
        }
    }

    selected
}

pub fn search_contexts(
    connection: &Connection,
    project_id: i64,
    question: &str,
    limit: usize,
) -> Result<Vec<SearchContext>, String> {
    let tokens = tokenize(question);
    let symbol_names = load_symbol_names(connection, project_id)?;
    let overview_question = is_overview_question(question);
    let seed_names = match_graph_seed_names(connection, project_id, &tokens)?;
    let graph_locations = expand_graph_locations(connection, project_id, &seed_names)?;
    let has_graph_hits = !graph_locations.is_empty();

    let mut statement = connection
        .prepare("SELECT file, start_line, end_line, text FROM chunks WHERE project_id = ?1")
        .map_err(sql_error)?;

    let rows = statement
        .query_map(params![project_id], |row| {
            Ok(SearchContext {
                file: row.get(0)?,
                start_line: row.get::<_, i64>(1)? as u64,
                end_line: row.get::<_, i64>(2)? as u64,
                text: row.get(3)?,
                score: 0,
            })
        })
        .map_err(sql_error)?;

    let mut ranked = Vec::new();
    for row in rows {
        let mut context = row.map_err(sql_error)?;
        context.score = score_context(&context, &tokens, &symbol_names);
        if overview_question {
            // Slightly prefer first chunk of each file for directory overview questions.
            if context.start_line <= 1 {
                context.score += 2;
            }
        }
        ranked.push(context);
    }

    // Structure-first: boost chunks covering symbol defs, call sites, callers, includes.
    apply_graph_boosts(&mut ranked, &graph_locations);

    let code_limit = if overview_question {
        limit.saturating_sub(1).max(1)
    } else {
        limit
    };
    let mut selected = if has_graph_hits {
        // Prefer high graph scores even if they concentrate in fewer directories.
        ranked.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then(left.file.cmp(&right.file))
                .then(left.start_line.cmp(&right.start_line))
        });
        ranked.into_iter().take(code_limit).collect::<Vec<_>>()
    } else {
        select_diverse_contexts(ranked, code_limit)
    };

    if overview_question || selected.iter().all(|item| item.score == 0) {
        if let Ok(layout) = build_project_layout_context(connection, project_id) {
            selected.insert(0, layout);
            selected.truncate(limit.max(1));
        }
    }

    // Ensure at least one snippet per top-level directory when asking overview questions.
    if overview_question {
        let mut have: HashSet<String> = selected
            .iter()
            .filter(|item| item.file != "<project-layout>")
            .map(|item| top_level_dir(&item.file))
            .collect();
        let mut statement = connection
            .prepare(
                "SELECT file, start_line, end_line, text FROM chunks
                 WHERE project_id = ?1 AND start_line = 1
                 ORDER BY file",
            )
            .map_err(sql_error)?;
        let starters = statement
            .query_map(params![project_id], |row| {
                Ok(SearchContext {
                    file: row.get(0)?,
                    start_line: row.get::<_, i64>(1)? as u64,
                    end_line: row.get::<_, i64>(2)? as u64,
                    text: row.get(3)?,
                    score: 1,
                })
            })
            .map_err(sql_error)?;
        for starter in starters {
            let context = starter.map_err(sql_error)?;
            let dir = top_level_dir(&context.file);
            if have.contains(&dir) {
                continue;
            }
            have.insert(dir);
            selected.push(context);
        }
        selected.truncate(limit.max(selected.len().min(12)));
    }

    Ok(selected)
}

fn load_symbol_names(connection: &Connection, project_id: i64) -> Result<HashSet<String>, String> {
    let mut statement = connection
        .prepare("SELECT DISTINCT lower(name) FROM symbols WHERE project_id = ?1")
        .map_err(sql_error)?;
    let rows = statement
        .query_map(params![project_id], |row| row.get::<_, String>(0))
        .map_err(sql_error)?;

    let mut names = HashSet::new();
    for row in rows {
        names.insert(row.map_err(sql_error)?);
    }
    Ok(names)
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, String> {
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![table],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    Ok(count > 0)
}

#[derive(Debug, Clone)]
struct GraphLocation {
    file: String,
    line: u64,
    boost: i64,
}

/// Match question tokens to symbol / call names (exact lowercase match).
fn match_graph_seed_names(
    connection: &Connection,
    project_id: i64,
    tokens: &[String],
) -> Result<Vec<String>, String> {
    if tokens.is_empty() {
        return Ok(Vec::new());
    }

    let mut seeds = HashSet::new();
    let mut symbol_statement = connection
        .prepare("SELECT DISTINCT name FROM symbols WHERE project_id = ?1 AND lower(name) = ?2")
        .map_err(sql_error)?;
    let mut call_statement = connection
        .prepare("SELECT DISTINCT name FROM calls WHERE project_id = ?1 AND lower(name) = ?2")
        .map_err(sql_error)?;

    for token in tokens {
        let symbol_rows = symbol_statement
            .query_map(params![project_id, token], |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        for row in symbol_rows {
            seeds.insert(row.map_err(sql_error)?);
        }

        let call_rows = call_statement
            .query_map(params![project_id, token], |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        for row in call_rows {
            seeds.insert(row.map_err(sql_error)?);
        }
    }

    Ok(seeds.into_iter().collect())
}

fn expand_graph_locations(
    connection: &Connection,
    project_id: i64,
    seed_names: &[String],
) -> Result<Vec<GraphLocation>, String> {
    let mut locations = Vec::new();
    if seed_names.is_empty() {
        return Ok(locations);
    }

    let mut seed_files = HashSet::new();
    let mut symbol_statement = connection
        .prepare(
            "SELECT file, line, is_definition FROM symbols
             WHERE project_id = ?1 AND lower(name) = lower(?2)",
        )
        .map_err(sql_error)?;
    let mut call_statement = connection
        .prepare(
            "SELECT file, line FROM calls
             WHERE project_id = ?1 AND lower(name) = lower(?2)",
        )
        .map_err(sql_error)?;
    let mut enclosing_statement = connection
        .prepare(
            "SELECT file, line FROM symbols
             WHERE project_id = ?1 AND file = ?2 AND is_definition = 1 AND line <= ?3
             ORDER BY line DESC LIMIT 1",
        )
        .map_err(sql_error)?;

    for name in seed_names {
        let symbol_rows = symbol_statement
            .query_map(params![project_id, name], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)? as u64,
                    row.get::<_, i64>(2)? != 0,
                ))
            })
            .map_err(sql_error)?;
        for row in symbol_rows {
            let (file, line, is_definition) = row.map_err(sql_error)?;
            seed_files.insert(file.clone());
            locations.push(GraphLocation {
                file,
                line,
                boost: if is_definition { 120 } else { 90 },
            });
        }

        let call_rows = call_statement
            .query_map(params![project_id, name], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
            })
            .map_err(sql_error)?;
        for row in call_rows {
            let (file, line) = row.map_err(sql_error)?;
            locations.push(GraphLocation {
                file: file.clone(),
                line,
                boost: 80,
            });

            let enclosing = enclosing_statement
                .query_row(params![project_id, file, line as i64], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
                })
                .optional()
                .map_err(sql_error)?;
            if let Some((caller_file, caller_line)) = enclosing {
                seed_files.insert(caller_file.clone());
                locations.push(GraphLocation {
                    file: caller_file,
                    line: caller_line,
                    boost: 70,
                });
            }
        }
    }

    if !seed_files.is_empty() {
        let mut include_statement = connection
            .prepare("SELECT file, line, resolved_path FROM includes WHERE project_id = ?1")
            .map_err(sql_error)?;
        let include_rows = include_statement
            .query_map(params![project_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)? as u64,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(sql_error)?;
        for row in include_rows {
            let (file, line, resolved_path) = row.map_err(sql_error)?;
            let touches_seed = seed_files.contains(&file)
                || (!resolved_path.is_empty() && seed_files.contains(&resolved_path));
            if touches_seed {
                locations.push(GraphLocation {
                    file,
                    line,
                    boost: 45,
                });
            }
        }
    }

    Ok(locations)
}

fn apply_graph_boosts(contexts: &mut [SearchContext], locations: &[GraphLocation]) {
    for context in contexts.iter_mut() {
        let mut boost = 0_i64;
        for location in locations {
            if location.file == context.file
                && location.line >= context.start_line
                && location.line <= context.end_line
            {
                boost = boost.max(location.boost);
            }
        }
        context.score += boost;
    }
}

const ENTRY_SYMBOL_NAMES: &[&str] = &[
    "main",
    "wmain",
    "WinMain",
    "wWinMain",
    "DllMain",
    "_tmain",
    "_tWinMain",
];

fn is_entry_symbol_name(name: &str) -> bool {
    ENTRY_SYMBOL_NAMES
        .iter()
        .any(|entry| entry.eq_ignore_ascii_case(name))
}

fn clamp_inspect_depth(depth: Option<u32>) -> u32 {
    depth.unwrap_or(2).clamp(1, 4)
}

#[derive(Debug, Clone)]
struct CallEdge {
    caller: String,
    caller_file: String,
    caller_line: u64,
    callee: String,
    via_file: String,
    via_line: u64,
}

fn symbol_hit_from_row(
    file: String,
    line: u64,
    kind: String,
    name: String,
    signature: String,
    is_definition: bool,
    confidence: String,
) -> SymbolHit {
    SymbolHit {
        name,
        file,
        line,
        kind,
        signature,
        is_definition,
        confidence,
    }
}

fn search_symbol_hits(
    connection: &Connection,
    project_id: i64,
    query: &str,
    limit: usize,
) -> Result<Vec<SymbolHit>, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }

    let mut statement = connection
        .prepare(
            "SELECT file, line, kind, name, signature, is_definition, confidence
             FROM symbols
             WHERE project_id = ?1
               AND (
                    lower(name) = lower(?2)
                    OR lower(name) LIKE lower(?2) || '%'
                    OR lower(name) LIKE '%' || lower(?2) || '%'
               )
             ORDER BY
                CASE
                    WHEN lower(name) = lower(?2) THEN 0
                    WHEN lower(name) LIKE lower(?2) || '%' THEN 1
                    ELSE 2
                END,
                is_definition DESC,
                length(name) ASC,
                file ASC,
                line ASC
             LIMIT ?3",
        )
        .map_err(sql_error)?;

    let rows = statement
        .query_map(params![project_id, trimmed, limit as i64], |row| {
            Ok(symbol_hit_from_row(
                row.get(0)?,
                row.get::<_, i64>(1)? as u64,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get::<_, i64>(5)? != 0,
                row.get(6)?,
            ))
        })
        .map_err(sql_error)?;

    let mut hits = Vec::new();
    for row in rows {
        hits.push(row.map_err(sql_error)?);
    }
    Ok(hits)
}

fn load_definitions_for_name(
    connection: &Connection,
    project_id: i64,
    name: &str,
) -> Result<Vec<SymbolHit>, String> {
    let mut statement = connection
        .prepare(
            "SELECT file, line, kind, name, signature, is_definition, confidence
             FROM symbols
             WHERE project_id = ?1 AND lower(name) = lower(?2)
             ORDER BY is_definition DESC, file ASC, line ASC
             LIMIT 20",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map(params![project_id, name], |row| {
            Ok(symbol_hit_from_row(
                row.get(0)?,
                row.get::<_, i64>(1)? as u64,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get::<_, i64>(5)? != 0,
                row.get(6)?,
            ))
        })
        .map_err(sql_error)?;
    let mut hits = Vec::new();
    for row in rows {
        hits.push(row.map_err(sql_error)?);
    }
    Ok(hits)
}

fn best_hit_for_name(
    connection: &Connection,
    project_id: i64,
    name: &str,
) -> Result<Option<SymbolHit>, String> {
    Ok(load_definitions_for_name(connection, project_id, name)?
        .into_iter()
        .next())
}

fn load_definition_lines_for_file(
    connection: &Connection,
    project_id: i64,
    file: &str,
) -> Result<Vec<(u64, SymbolHit)>, String> {
    let mut statement = connection
        .prepare(
            "SELECT file, line, kind, name, signature, is_definition, confidence
             FROM symbols
             WHERE project_id = ?1 AND file = ?2 AND is_definition = 1
             ORDER BY line ASC",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map(params![project_id, file], |row| {
            Ok(symbol_hit_from_row(
                row.get(0)?,
                row.get::<_, i64>(1)? as u64,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get::<_, i64>(5)? != 0,
                row.get(6)?,
            ))
        })
        .map_err(sql_error)?;
    let mut items = Vec::new();
    for row in rows {
        let hit = row.map_err(sql_error)?;
        items.push((hit.line, hit));
    }
    Ok(items)
}

fn enclosing_from_defs(defs: &[(u64, SymbolHit)], line: u64) -> Option<&SymbolHit> {
    let mut best: Option<&SymbolHit> = None;
    for (def_line, hit) in defs {
        if *def_line <= line {
            best = Some(hit);
        } else {
            break;
        }
    }
    best
}

/// Incoming edges: who calls `callee` (query by call site name — O(matches), not whole project).
fn load_edges_to_callee(
    connection: &Connection,
    project_id: i64,
    callee: &str,
) -> Result<Vec<CallEdge>, String> {
    let mut statement = connection
        .prepare(
            "SELECT file, line, name FROM calls
             WHERE project_id = ?1 AND lower(name) = lower(?2)
             LIMIT 200",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map(params![project_id, callee], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)? as u64,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(sql_error)?;

    let mut by_file: BTreeMap<String, Vec<(u64, String)>> = BTreeMap::new();
    for row in rows {
        let (via_file, via_line, callee_name) = row.map_err(sql_error)?;
        by_file
            .entry(via_file)
            .or_default()
            .push((via_line, callee_name));
    }

    let mut edges = Vec::new();
    for (via_file, calls) in by_file {
        let defs = load_definition_lines_for_file(connection, project_id, &via_file)?;
        for (via_line, callee_name) in calls {
            let Some(caller) = enclosing_from_defs(&defs, via_line) else {
                continue;
            };
            if caller.name.eq_ignore_ascii_case(&callee_name) {
                continue;
            }
            edges.push(CallEdge {
                caller: caller.name.clone(),
                caller_file: caller.file.clone(),
                caller_line: caller.line,
                callee: callee_name,
                via_file: via_file.clone(),
                via_line,
            });
        }
    }
    Ok(edges)
}

/// Outgoing edges: calls made from `caller` (scan call sites in its definition files only).
fn load_edges_from_caller(
    connection: &Connection,
    project_id: i64,
    caller: &str,
) -> Result<Vec<CallEdge>, String> {
    let definitions = load_definitions_for_name(connection, project_id, caller)?;
    let mut files = HashSet::new();
    for definition in &definitions {
        if definition.is_definition {
            files.insert(definition.file.clone());
        }
    }
    if files.is_empty() {
        for definition in &definitions {
            files.insert(definition.file.clone());
        }
    }

    let mut edges = Vec::new();
    let mut statement = connection
        .prepare("SELECT line, name FROM calls WHERE project_id = ?1 AND file = ?2 LIMIT 2000")
        .map_err(sql_error)?;

    for file in files {
        let defs = load_definition_lines_for_file(connection, project_id, &file)?;
        let rows = statement
            .query_map(params![project_id, file], |row| {
                Ok((row.get::<_, i64>(0)? as u64, row.get::<_, String>(1)?))
            })
            .map_err(sql_error)?;
        for row in rows {
            let (via_line, callee) = row.map_err(sql_error)?;
            let Some(enclosing) = enclosing_from_defs(&defs, via_line) else {
                continue;
            };
            if !enclosing.name.eq_ignore_ascii_case(caller) {
                continue;
            }
            if enclosing.name.eq_ignore_ascii_case(&callee) {
                continue;
            }
            edges.push(CallEdge {
                caller: enclosing.name.clone(),
                caller_file: enclosing.file.clone(),
                caller_line: enclosing.line,
                callee,
                via_file: file.clone(),
                via_line,
            });
        }
    }
    Ok(edges)
}

fn collect_impact_neighbors(
    connection: &Connection,
    project_id: i64,
    symbol: &str,
    want_callers: bool,
    depth: u32,
    cap: usize,
) -> Result<Vec<ImpactNeighbor>, String> {
    let mut results = Vec::new();
    let mut seen_names = HashSet::new();
    let mut frontier: HashSet<String> = HashSet::new();
    frontier.insert(symbol.to_string());

    for _ in 0..depth {
        let mut next_frontier = HashSet::new();
        for current in &frontier {
            let edges = if want_callers {
                load_edges_to_callee(connection, project_id, current)?
            } else {
                load_edges_from_caller(connection, project_id, current)?
            };
            for edge in edges {
                let (neighbor_name, neighbor_file, neighbor_line) = if want_callers {
                    (
                        edge.caller.clone(),
                        edge.caller_file.clone(),
                        edge.caller_line,
                    )
                } else {
                    (edge.callee.clone(), edge.via_file.clone(), edge.via_line)
                };

                let key = neighbor_name.to_lowercase();
                if key == symbol.to_lowercase() || !seen_names.insert(key.clone()) {
                    continue;
                }
                next_frontier.insert(neighbor_name.clone());
                results.push(ImpactNeighbor {
                    name: neighbor_name,
                    file: neighbor_file,
                    line: neighbor_line,
                    via_file: edge.via_file.clone(),
                    via_line: edge.via_line,
                });
                if results.len() >= cap {
                    return Ok(results);
                }
            }
        }
        if next_frontier.is_empty() {
            break;
        }
        frontier = next_frontier;
    }

    Ok(results)
}

fn find_entry_paths(
    connection: &Connection,
    project_id: i64,
    target: &str,
    max_paths: usize,
    max_depth: usize,
) -> Result<Vec<EntryPath>, String> {
    let mut queue = VecDeque::new();
    queue.push_back((target.to_string(), vec![target.to_string()]));
    let mut visited = HashSet::new();
    visited.insert(target.to_lowercase());
    let mut paths = Vec::new();
    let mut expansions = 0_usize;
    const MAX_EXPANSIONS: usize = 64;

    while let Some((current, chain)) = queue.pop_front() {
        if paths.len() >= max_paths {
            break;
        }
        if chain.len() > max_depth {
            continue;
        }
        if is_entry_symbol_name(&current) && chain.len() > 1 {
            let mut hops = Vec::new();
            for name in chain.iter().rev() {
                if let Some(hit) = best_hit_for_name(connection, project_id, name)? {
                    hops.push(hit);
                } else {
                    hops.push(SymbolHit {
                        name: name.clone(),
                        file: String::new(),
                        line: 0,
                        kind: "function".to_string(),
                        signature: String::new(),
                        is_definition: false,
                        confidence: "heuristic".to_string(),
                    });
                }
            }
            paths.push(EntryPath {
                entry: current,
                hops,
            });
            continue;
        }

        if expansions >= MAX_EXPANSIONS {
            break;
        }
        expansions += 1;

        let parents = load_edges_to_callee(connection, project_id, &current)?;
        for edge in parents {
            let key = edge.caller.to_lowercase();
            if !visited.insert(key) {
                continue;
            }
            let mut next_chain = chain.clone();
            next_chain.push(edge.caller.clone());
            queue.push_back((edge.caller.clone(), next_chain));
        }
    }

    Ok(paths)
}

fn resolve_inspect_target(hits: &[SymbolHit], query: &str) -> Option<String> {
    let exact: Vec<&SymbolHit> = hits
        .iter()
        .filter(|hit| hit.name.eq_ignore_ascii_case(query))
        .collect();
    if !exact.is_empty() {
        return Some(exact[0].name.clone());
    }

    let mut names = HashSet::new();
    for hit in hits {
        names.insert(hit.name.to_lowercase());
    }
    if names.len() == 1 {
        return hits.first().map(|hit| hit.name.clone());
    }
    None
}

pub fn inspect_symbol_at(
    connection: &Connection,
    project_id: i64,
    query: &str,
    depth: Option<u32>,
) -> Result<InspectSymbolResponse, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(InspectSymbolResponse {
            query: trimmed.to_string(),
            message: "请输入要查询的符号名。".to_string(),
            candidates: Vec::new(),
            report: None,
        });
    }

    let depth = clamp_inspect_depth(depth);
    let candidates = search_symbol_hits(connection, project_id, trimmed, 24)?;
    if candidates.is_empty() {
        return Ok(InspectSymbolResponse {
            query: trimmed.to_string(),
            message: format!("未找到与 `{trimmed}` 匹配的符号。可先重新索引，或换更短的标识符。"),
            candidates: Vec::new(),
            report: None,
        });
    }

    let Some(target_name) = resolve_inspect_target(&candidates, trimmed) else {
        return Ok(InspectSymbolResponse {
            query: trimmed.to_string(),
            message: format!(
                "找到多个候选符号，请输入更精确的名称（当前显示前 {} 个）。",
                candidates.len()
            ),
            candidates,
            report: None,
        });
    };

    let definitions = load_definitions_for_name(connection, project_id, &target_name)?;
    let callers = collect_impact_neighbors(connection, project_id, &target_name, true, depth, 40)?;
    let mut callees =
        collect_impact_neighbors(connection, project_id, &target_name, false, depth, 40)?;
    for callee in &mut callees {
        if let Some(hit) = best_hit_for_name(connection, project_id, &callee.name)? {
            callee.file = hit.file;
            callee.line = hit.line;
        }
    }
    let entry_paths = find_entry_paths(connection, project_id, &target_name, 3, 10)?;

    let message = format!(
        "符号 `{target_name}`：{} 个定义，{} 个上游，{} 个下游，{} 条入口路径。",
        definitions.len(),
        callers.len(),
        callees.len(),
        entry_paths.len()
    );

    Ok(InspectSymbolResponse {
        query: trimmed.to_string(),
        message,
        candidates: definitions.clone(),
        report: Some(ImpactReport {
            symbol: target_name,
            definitions,
            callers,
            callees,
            entry_paths,
            confidence: "heuristic".to_string(),
        }),
    })
}

pub fn tokenize(question: &str) -> Vec<String> {
    let mut tokens = HashSet::new();
    // Prefer ASCII identifiers — they match source paths/symbols. Pure CJK phrases rarely help.
    for raw in
        question.split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
    {
        let token = raw.trim().to_lowercase();
        if token.len() >= 3 {
            tokens.insert(token);
        }
    }
    tokens.into_iter().collect()
}

pub fn score_context(
    context: &SearchContext,
    tokens: &[String],
    symbol_names: &HashSet<String>,
) -> i64 {
    let file = context.file.to_lowercase();
    let text = context.text.to_lowercase();
    let mut score = 0_i64;

    for token in tokens {
        if file.contains(token) {
            score += 18;
        }
        if symbol_names.contains(token) {
            score += 10;
        }
        score += text.matches(token).count() as i64;
        if text.contains(&format!(" {token}(")) || text.contains(&format!("::{token}")) {
            score += 6;
        }
    }

    if file.contains("main.") {
        score += 4;
    }

    score
}

fn contexts_to_citations(contexts: &[SearchContext]) -> Vec<Citation> {
    contexts
        .iter()
        .map(|context| Citation {
            file: context.file.clone(),
            line: context.start_line,
            title: format!(
                "{}:{}-{}",
                context.file, context.start_line, context.end_line
            ),
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct OpenAiSettings {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicOpenAiSettings {
    pub base_url: String,
    pub model: String,
    pub api_key_configured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateOpenAiSettings {
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    #[serde(default)]
    pub clear_api_key: bool,
}

pub fn public_openai_settings(settings: &OpenAiSettings) -> PublicOpenAiSettings {
    PublicOpenAiSettings {
        base_url: settings.base_url.clone(),
        model: settings.model.clone(),
        api_key_configured: !settings.api_key.trim().is_empty(),
    }
}

impl Default for OpenAiSettings {
    fn default() -> Self {
        Self {
            base_url: default_openai_base_url(),
            api_key: String::new(),
            model: default_openai_model(),
        }
    }
}

pub fn default_openai_base_url() -> String {
    "https://api.openai.com/v1".to_string()
}

pub fn default_openai_model() -> String {
    "gpt-4o-mini".to_string()
}

fn read_setting_value(connection: &Connection, key: &str) -> Result<Option<String>, String> {
    let value: Option<String> = connection
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    Ok(value.and_then(|item| {
        let trimmed = item.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }))
}

fn upsert_setting_value(connection: &Connection, key: &str, value: &str) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO settings(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .map_err(sql_error)?;
    Ok(())
}

pub fn read_openai_api_key(connection: &Connection) -> Result<Option<String>, String> {
    let settings = read_openai_settings(connection)?;
    if settings.api_key.is_empty() {
        Ok(None)
    } else {
        Ok(Some(settings.api_key))
    }
}

pub fn read_openai_settings(connection: &Connection) -> Result<OpenAiSettings, String> {
    read_openai_settings_with_backend(connection, &secrets::OsSecretBackend)
}

pub fn read_openai_settings_with_backend(
    connection: &Connection,
    backend: &dyn secrets::SecretBackend,
) -> Result<OpenAiSettings, String> {
    secrets::migrate_legacy_api_key(connection, backend)?;
    let mut settings = OpenAiSettings::default();

    if let Ok(value) =
        std::env::var("OPENAI_BASE_URL").or_else(|_| std::env::var("OPENAI_API_BASE"))
    {
        let trimmed = value.trim().trim_end_matches('/').to_string();
        if !trimmed.is_empty() {
            settings.base_url = trimmed;
        }
    } else if let Some(value) = read_setting_value(connection, "openai_base_url")? {
        settings.base_url = value.trim_end_matches('/').to_string();
    }

    if let Ok(value) = std::env::var("OPENAI_API_KEY") {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            settings.api_key = trimmed;
        }
    } else if let Some(value) = backend.get(secrets::OPENAI_ACCOUNT)? {
        settings.api_key = value;
    }

    if let Ok(value) = std::env::var("OPENAI_MODEL") {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            settings.model = trimmed;
        }
    } else if let Some(value) = read_setting_value(connection, "openai_model")? {
        settings.model = value;
    }

    Ok(settings)
}

pub fn set_api_key_at(db_path: &Path, api_key: &str) -> Result<(), String> {
    let connection = open_database_at(db_path)?;
    let backend = secrets::OsSecretBackend;
    secrets::migrate_legacy_api_key(&connection, &backend)?;
    secrets::update_api_key(&backend, Some(api_key), false)
}

pub fn set_openai_settings_at(
    db_path: &Path,
    settings: &UpdateOpenAiSettings,
) -> Result<(), String> {
    let connection = open_database_at(db_path)?;
    let backend = secrets::OsSecretBackend;
    secrets::migrate_legacy_api_key(&connection, &backend)?;
    let base_url = settings.base_url.trim().trim_end_matches('/');
    let base_url = if base_url.is_empty() {
        default_openai_base_url()
    } else {
        base_url.to_string()
    };
    let model = settings.model.trim();
    let model = if model.is_empty() {
        default_openai_model()
    } else {
        model.to_string()
    };

    upsert_setting_value(&connection, "openai_base_url", &base_url)?;
    upsert_setting_value(&connection, "openai_model", &model)?;
    secrets::update_api_key(
        &backend,
        settings.api_key.as_deref(),
        settings.clear_api_key,
    )?;
    Ok(())
}

pub fn chat_completions_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/chat/completions")
    }
}

pub fn list_projects(db_path: &Path) -> Result<Vec<(i64, String, String)>, String> {
    let connection = open_database_at(db_path)?;
    let mut statement = connection
        .prepare("SELECT id, root, indexed_at FROM projects ORDER BY id DESC")
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(sql_error)?;

    let mut projects = Vec::new();
    for row in rows {
        projects.push(row.map_err(sql_error)?);
    }
    Ok(projects)
}

fn find_project_id_normalized(connection: &Connection, root: &str) -> Result<Option<i64>, String> {
    let target = normalize_root_key(root);
    let mut statement = connection
        .prepare("SELECT id, root FROM projects ORDER BY id DESC")
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(sql_error)?;
    for row in rows {
        let (id, stored_root) = row.map_err(sql_error)?;
        if normalize_root_key(&stored_root) == target {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

pub fn find_project_id_by_root(db_path: &Path, root: &str) -> Result<i64, String> {
    let connection = open_database_at(db_path)?;
    if let Some(id) = find_project_id_normalized(&connection, root)? {
        return Ok(id);
    }
    Err(format!("no indexed project found for root: {root}"))
}

/// Load overview from an existing project-local index, if present.
pub fn try_load_indexed_project_at(
    root: &Path,
    db_path: &Path,
) -> Result<Option<ProjectOverview>, String> {
    if !db_path.exists() {
        return Ok(None);
    }
    let root_str = root.to_string_lossy().to_string();
    let connection = open_database_at(db_path)?;

    if let Some(project_id) = find_project_id_normalized(&connection, &root_str)? {
        return Ok(Some(load_project_overview(&connection, project_id)?));
    }

    // Project-local DB lives under `<root>/.cppinsight/`; if it only has one project row,
    // treat it as a cache hit even when stored root string differs slightly.
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
        .map_err(sql_error)?;
    if count == 1 {
        let project_id: i64 = connection
            .query_row(
                "SELECT id FROM projects ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        return Ok(Some(load_project_overview(&connection, project_id)?));
    }

    Ok(None)
}

pub fn build_client_summary(connection: &Connection, project_id: i64) -> Result<String, String> {
    let overview = load_project_overview(connection, project_id)?;
    let mut output = String::new();
    output.push_str("# CppInsight Client Brief\n\n");
    output.push_str(&format!("Project: {}\n\n", overview.root));
    output.push_str("## Scale\n");
    output.push_str(&format!("- Indexed C/C++ files: {}\n", overview.file_count));
    output.push_str(&format!("- Code lines: {}\n", overview.stats.lines.code));
    output.push_str(&format!("- Symbols: {}\n", overview.symbol_count));
    output.push_str(&format!("- Analysis mode: {}\n\n", overview.mode));

    output.push_str("## Likely structure\n");
    let mut modules: Vec<_> = overview.second_level_layout.iter().collect();
    modules.sort_by(|left, right| right.1.cmp(left.1).then(left.0.cmp(right.0)));
    for (path, count) in modules.into_iter().take(8) {
        output.push_str(&format!("- `{path}`: {count} files\n"));
    }
    output.push('\n');

    output.push_str("## Hot files\n");
    for hotspot in overview.hotspots.iter().take(6) {
        output.push_str(&format!(
            "- `{}` ({} code lines, {} symbols)\n",
            hotspot.path, hotspot.code_lines, hotspot.symbol_count
        ));
    }
    output.push('\n');

    output.push_str("## Caveat\n");
    output.push_str("Generated for onboarding/estimation. Validate before committing scope.\n");
    Ok(output)
}

fn looks_like_sse(raw: &str) -> bool {
    raw.lines()
        .any(|line| line.trim_start().starts_with("data:"))
}

fn extract_message_content(value: &serde_json::Value) -> Option<String> {
    value["choices"][0]["message"]["content"]
        .as_str()
        .map(|content| content.to_string())
        .filter(|content| !content.is_empty())
}

/// Parse OpenAI chat.completion JSON or SSE (`data: {...}`) streams into assistant text.
pub fn parse_chat_completion_body(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("empty response body".to_string());
    }

    if !looks_like_sse(trimmed) {
        let value: serde_json::Value = serde_json::from_str(trimmed)
            .map_err(|error| format!("invalid JSON response: {error}"))?;
        return extract_message_content(&value)
            .ok_or_else(|| format!("OpenAI response did not include message content: {value}"));
    }

    let mut content = String::new();
    let mut reasoning = String::new();
    for line in trimmed.lines() {
        let line = line.trim();
        let Some(payload) = line.strip_prefix("data:") else {
            continue;
        };
        let payload = payload.trim();
        if payload.is_empty() || payload == "[DONE]" {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) else {
            continue;
        };
        if let Some(piece) = value["choices"][0]["delta"]["content"].as_str() {
            content.push_str(piece);
        }
        if let Some(piece) = value["choices"][0]["delta"]["reasoning_content"].as_str() {
            reasoning.push_str(piece);
        }
        if let Some(piece) = value["choices"][0]["message"]["content"].as_str() {
            if !piece.is_empty() {
                content = piece.to_string();
            }
        }
    }

    if !content.trim().is_empty() {
        return Ok(content);
    }
    if !reasoning.trim().is_empty() {
        // Some gateways stream only reasoning_content for a while; surface it rather than failing.
        return Ok(reasoning);
    }
    Err("SSE response did not include assistant content".to_string())
}

async fn call_openai(
    settings: &OpenAiSettings,
    question: &str,
    contexts: &[SearchContext],
) -> Result<String, String> {
    let context_text = contexts
        .iter()
        .map(|context| {
            format!(
                "[{}:{}-{}]\n{}",
                context.file, context.start_line, context.end_line, context.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let body = json!({
        "model": settings.model,
        "temperature": 0.2,
        "stream": false,
        "messages": [
            {
                "role": "system",
                "content": "You help C++ developers understand unfamiliar codebases. Answer only from the provided project context. Cite file paths and line ranges when relevant. If the context is insufficient, say what is missing."
            },
            {
                "role": "user",
                "content": format!("Project context:\n{}\n\nQuestion:\n{}", context_text, question)
            }
        ]
    });

    let endpoint = chat_completions_url(&settings.base_url);
    let response = reqwest::Client::new()
        .post(&endpoint)
        .bearer_auth(&settings.api_key)
        .header("Accept", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("OpenAI request failed ({endpoint}): {error}"))?;

    let status = response.status();
    let raw = response
        .text()
        .await
        .map_err(|error| format!("failed to read OpenAI response body ({endpoint}): {error}"))?;

    if !status.is_success() {
        let preview: String = raw.chars().take(240).collect();
        return Err(format!(
            "OpenAI request failed with {status} ({endpoint}). body preview: {preview}"
        ));
    }

    parse_chat_completion_body(&raw).map_err(|error| {
        let preview: String = raw.chars().take(240).collect();
        format!("{error} ({endpoint}). body preview: {preview}")
    })
}

pub(crate) fn sql_error(error: rusqlite::Error) -> String {
    format!("SQLite error: {error}")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::scan_project,
            commands::index_project,
            commands::index_scan_json,
            commands::get_project_overview,
            commands::load_indexed_project,
            commands::load_workspace,
            commands::save_workspace,
            commands::load_project_chats,
            commands::get_openai_settings,
            commands::set_openai_settings,
            commands::preview_diagnostics,
            commands::export_diagnostics,
            commands::ask_project,
            commands::export_client_summary,
            commands::inspect_symbol
        ])
        .run(tauri::generate_context!())
        .expect("error while running CppInsight AI");
}

#[cfg(test)]
mod tests {
    use super::{
        parse_scan_progress_line, public_openai_settings, read_openai_settings_with_backend,
        score_context, tokenize, OpenAiSettings, SearchContext,
    };
    use crate::secrets::SecretBackend;
    use std::collections::HashMap;
    use std::collections::{BTreeMap, HashSet};
    use std::sync::Mutex;

    #[derive(Default)]
    struct TestSecretBackend {
        values: Mutex<HashMap<String, String>>,
    }

    impl crate::secrets::SecretBackend for TestSecretBackend {
        fn get(&self, account: &str) -> Result<Option<String>, String> {
            Ok(self.values.lock().unwrap().get(account).cloned())
        }

        fn set(&self, account: &str, value: &str) -> Result<(), String> {
            self.values
                .lock()
                .unwrap()
                .insert(account.to_string(), value.to_string());
            Ok(())
        }

        fn delete(&self, account: &str) -> Result<(), String> {
            self.values.lock().unwrap().remove(account);
            Ok(())
        }
    }

    #[test]
    fn reading_ai_settings_migrates_plaintext_and_reads_secret_backend() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        super::migrate_database(&connection).unwrap();
        super::upsert_setting_value(&connection, "openai_api_key", "sk-legacy").unwrap();
        let backend = TestSecretBackend::default();

        let settings = read_openai_settings_with_backend(&connection, &backend).unwrap();

        assert_eq!(settings.api_key, "sk-legacy");
        assert_eq!(
            super::read_setting_value(&connection, "openai_api_key").unwrap(),
            None
        );
    }

    #[test]
    fn legacy_database_migration_moves_secret_directly_to_backend() {
        let legacy = rusqlite::Connection::open_in_memory().unwrap();
        let settings = rusqlite::Connection::open_in_memory().unwrap();
        super::migrate_database(&legacy).unwrap();
        super::migrate_database(&settings).unwrap();
        super::upsert_setting_value(&legacy, "openai_api_key", "sk-old-global").unwrap();
        let backend = TestSecretBackend::default();

        super::migrate_api_key_once_with_backend(&legacy, &settings, &backend).unwrap();

        assert_eq!(
            backend
                .get(crate::secrets::OPENAI_ACCOUNT)
                .unwrap()
                .as_deref(),
            Some("sk-old-global")
        );
        assert_eq!(
            super::read_setting_value(&legacy, "openai_api_key").unwrap(),
            None
        );
        assert_eq!(
            super::read_setting_value(&settings, "openai_api_key").unwrap(),
            None
        );
    }

    #[test]
    fn restored_workspace_migrates_legacy_secret_without_project_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let legacy_path = directory.path().join("projects.sqlite");
        let legacy = rusqlite::Connection::open(&legacy_path).unwrap();
        super::migrate_database(&legacy).unwrap();
        super::upsert_setting_value(&legacy, "openai_api_key", "sk-restored").unwrap();
        drop(legacy);

        let settings = rusqlite::Connection::open_in_memory().unwrap();
        super::migrate_database(&settings).unwrap();
        let backend = TestSecretBackend::default();

        super::migrate_legacy_api_keys_from_paths(
            &settings,
            std::slice::from_ref(&legacy_path),
            &backend,
        )
        .unwrap();

        assert_eq!(
            backend
                .get(crate::secrets::OPENAI_ACCOUNT)
                .unwrap()
                .as_deref(),
            Some("sk-restored")
        );
        let legacy = rusqlite::Connection::open(legacy_path).unwrap();
        assert_eq!(
            super::read_setting_value(&legacy, "openai_api_key").unwrap(),
            None
        );
    }

    #[test]
    fn legacy_secret_candidates_preserve_appdata_priority_when_keys_conflict() {
        let directory = tempfile::tempdir().unwrap();
        let appdata_path = directory.path().join("z-appdata.sqlite");
        let cli_path = directory.path().join("a-cli.sqlite");
        for (path, key) in [
            (&appdata_path, "sk-desktop-newer"),
            (&cli_path, "sk-cli-older"),
        ] {
            let connection = rusqlite::Connection::open(path).unwrap();
            super::migrate_database(&connection).unwrap();
            super::upsert_setting_value(&connection, "openai_api_key", key).unwrap();
        }

        let candidates = super::deduplicate_legacy_candidates(vec![
            appdata_path.clone(),
            cli_path.clone(),
            appdata_path.clone(),
        ]);
        assert_eq!(candidates, vec![appdata_path.clone(), cli_path.clone()]);

        let settings = rusqlite::Connection::open_in_memory().unwrap();
        super::migrate_database(&settings).unwrap();
        let backend = TestSecretBackend::default();
        super::migrate_legacy_api_keys_from_paths(&settings, &candidates, &backend).unwrap();

        assert_eq!(
            backend
                .get(crate::secrets::OPENAI_ACCOUNT)
                .unwrap()
                .as_deref(),
            Some("sk-desktop-newer")
        );
        for path in [appdata_path, cli_path] {
            let connection = rusqlite::Connection::open(path).unwrap();
            assert_eq!(
                super::read_setting_value(&connection, "openai_api_key").unwrap(),
                None
            );
        }
    }

    #[test]
    fn public_ai_settings_report_configuration_without_serializing_secret() {
        let public = public_openai_settings(&OpenAiSettings {
            base_url: "https://example.test/v1".to_string(),
            api_key: "sk-never-return-this".to_string(),
            model: "test-model".to_string(),
        });
        let json = serde_json::to_string(&public).unwrap();

        assert!(public.api_key_configured);
        assert!(!json.contains("sk-never-return-this"));
        assert!(!json.contains("api_key\""));
    }

    #[test]
    fn scores_file_symbol_and_text_matches() {
        let tokens = tokenize("Where is ProjectScanner implemented?");
        let mut symbols = HashSet::new();
        symbols.insert("projectscanner".to_string());
        let context = SearchContext {
            file: "src/project/ProjectScanner.cpp".to_string(),
            start_line: 1,
            end_line: 12,
            text: "ProjectScanner scans C++ files".to_string(),
            score: 0,
        };

        assert!(score_context(&context, &tokens, &symbols) > 0);
    }

    #[test]
    fn parses_scan_progress_line() {
        let parsed = parse_scan_progress_line(
            "[scan] files=1200 analyzed=800 reused=100 current=src/foo.cpp",
        )
        .expect("should parse");
        assert_eq!(parsed.files_discovered, 1200);
        assert_eq!(parsed.files_analyzed, 800);
        assert_eq!(parsed.files_reused, 100);
        assert_eq!(parsed.current_file, "src/foo.cpp");
    }

    #[test]
    fn ignores_invalid_scan_progress_line() {
        assert!(parse_scan_progress_line("not a progress line").is_none());
        assert!(
            parse_scan_progress_line("[scan] files=abc analyzed=1 reused=0 current=x").is_none()
        );
    }

    #[test]
    fn summarizes_cli_stderr_prefers_error_lines() {
        let lines = vec![
            "[scan] files=1 analyzed=0 reused=0 current=a.cpp".to_string(),
            "[ERROR] Project path does not exist: D:/missing".to_string(),
        ];
        let summary = super::summarize_cli_stderr(&lines);
        assert!(summary.contains("Project path does not exist"));
    }

    #[test]
    fn project_index_db_path_is_under_cppinsight_dir() {
        let path = super::project_index_db_path(std::path::Path::new("D:/repos/demo"));
        assert!(
            path.ends_with(std::path::Path::new(".cppinsight/index.sqlite"))
                || path.ends_with(std::path::Path::new(".cppinsight\\index.sqlite"))
        );
    }

    #[test]
    fn builds_chat_completions_url_from_base() {
        assert_eq!(
            super::chat_completions_url("https://api.openai.com/v1"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            super::chat_completions_url("https://proxy.example.com/v1/"),
            "https://proxy.example.com/v1/chat/completions"
        );
        assert_eq!(
            super::chat_completions_url("https://proxy.example.com/v1/chat/completions"),
            "https://proxy.example.com/v1/chat/completions"
        );
    }

    #[test]
    fn parses_non_streaming_chat_completion() {
        let raw = r#"{"choices":[{"message":{"role":"assistant","content":"入口在 main"}}]}"#;
        assert_eq!(
            super::parse_chat_completion_body(raw).unwrap(),
            "入口在 main"
        );
    }

    #[test]
    fn parses_sse_chat_completion_chunks() {
        let raw = r#"
data: {"choices":[{"delta":{"role":"assistant","content":""}}]}

data: {"choices":[{"delta":{"content":"入口"}}]}

data: {"choices":[{"delta":{"content":"在 main"}}]}

data: [DONE]
"#;
        assert_eq!(
            super::parse_chat_completion_body(raw).unwrap(),
            "入口在 main"
        );
    }

    #[test]
    fn detects_overview_questions() {
        assert!(super::is_overview_question(
            "这个项目的顶层目录分别大概负责什么？"
        ));
        assert!(super::is_overview_question("project module layout"));
        assert!(!super::is_overview_question(
            "Where is encryptFile implemented?"
        ));
    }

    #[test]
    fn diversifies_contexts_across_top_level_dirs() {
        let ranked = vec![
            super::SearchContext {
                file: "dsps/a.cpp".into(),
                start_line: 1,
                end_line: 10,
                text: "a".into(),
                score: 5,
            },
            super::SearchContext {
                file: "dsps/b.cpp".into(),
                start_line: 1,
                end_line: 10,
                text: "b".into(),
                score: 4,
            },
            super::SearchContext {
                file: "dsps/c.cpp".into(),
                start_line: 1,
                end_line: 10,
                text: "c".into(),
                score: 3,
            },
            super::SearchContext {
                file: "utils/u.cpp".into(),
                start_line: 1,
                end_line: 10,
                text: "u".into(),
                score: 2,
            },
        ];
        let selected = super::select_diverse_contexts(ranked, 3);
        let dirs: HashSet<_> = selected
            .iter()
            .map(|item| super::top_level_dir(&item.file))
            .collect();
        assert!(dirs.contains("dsps"));
        assert!(dirs.contains("utils"));
    }

    fn open_memory_db() -> rusqlite::Connection {
        let connection = rusqlite::Connection::open_in_memory().expect("memory db");
        super::migrate_database(&connection).expect("migrate");
        connection
    }

    #[test]
    fn try_load_indexed_project_returns_none_without_db() {
        let root = std::env::temp_dir().join(format!("cppinsight-missing-{}", std::process::id()));
        let db = root.join(".cppinsight").join("index.sqlite");
        let loaded = super::try_load_indexed_project_at(&root, &db).expect("load");
        assert!(loaded.is_none());
    }

    #[test]
    fn try_load_indexed_project_reads_existing_index() {
        let root = std::env::temp_dir().join(format!("cppinsight-load-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".cppinsight")).expect("mkdir");
        let db_path = root.join(".cppinsight").join("index.sqlite");
        let root_str = root.to_string_lossy().to_string();

        {
            let mut connection = super::open_database_at(&db_path).expect("open db");
            let scan = super::ScanResult {
                schema_version: 1,
                generator_version: String::new(),
                root: root_str.clone(),
                mode: "heuristic".into(),
                confidence: "heuristic".into(),
                scan_duration_ms: 0,
                compile_commands: String::new(),
                compile_commands_entries: 0,
                reused_file_count: 0,
                file_count: 1,
                skipped_entries: 0,
                stats: super::ScanStats::default(),
                extensions: BTreeMap::new(),
                top_level_layout: BTreeMap::new(),
                second_level_layout: BTreeMap::new(),
                diagnostics: Vec::new(),
                includes: Vec::new(),
                symbols: Vec::new(),
                calls: Vec::new(),
                relations: Vec::new(),
                entry_points: Vec::new(),
                files: Vec::new(),
            };
            super::write_index(&mut connection, &scan).expect("write_index");
        }

        let loaded = super::try_load_indexed_project_at(&root, &db_path)
            .expect("load")
            .expect("should find project");
        assert!(loaded.project_id > 0);
        assert_eq!(loaded.root, root_str);

        // Slash/case variants should still hit the same project-local index.
        let alt = root_str.replace('\\', "/");
        let loaded_alt = super::try_load_indexed_project_at(std::path::Path::new(&alt), &db_path)
            .expect("load alt")
            .expect("slash variant should match");
        assert_eq!(loaded_alt.project_id, loaded.project_id);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn write_index_persists_calls() {
        let mut connection = open_memory_db();
        let scan = super::ScanResult {
            schema_version: 1,
            generator_version: String::new(),
            root: "D:/demo".into(),
            mode: "heuristic".into(),
            confidence: "heuristic".into(),
            scan_duration_ms: 0,
            compile_commands: String::new(),
            compile_commands_entries: 0,
            reused_file_count: 0,
            file_count: 0,
            skipped_entries: 0,
            stats: super::ScanStats::default(),
            extensions: BTreeMap::new(),
            top_level_layout: BTreeMap::new(),
            second_level_layout: BTreeMap::new(),
            diagnostics: Vec::new(),
            includes: Vec::new(),
            symbols: vec![super::SymbolInfo {
                file: "src/a.cpp".into(),
                line: 10,
                kind: "function".into(),
                name: "encryptFile".into(),
                qualified_name: String::new(),
                stable_key: String::new(),
                parent_qualified_name: String::new(),
                parent_stable_key: String::new(),
                signature: "void encryptFile()".into(),
                scope: String::new(),
                is_definition: true,
                confidence: "heuristic".into(),
            }],
            calls: vec![super::CallInfo {
                file: "src/b.cpp".into(),
                line: 42,
                name: "encryptFile".into(),
                target_qualified_name: String::new(),
                caller_qualified_name: String::new(),
                caller_stable_key: String::new(),
                confidence: "heuristic".into(),
            }],
            relations: Vec::new(),
            entry_points: Vec::new(),
            files: Vec::new(),
        };

        let project_id = super::write_index(&mut connection, &scan).expect("write_index");
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM calls WHERE project_id = ?1 AND name = 'encryptFile'",
                rusqlite::params![project_id],
                |row| row.get(0),
            )
            .expect("count calls");
        assert_eq!(count, 1);
    }

    #[test]
    fn legacy_project_copy_backfills_stable_keys_and_complete_generation() {
        let source = open_memory_db();
        source
            .execute(
                "INSERT INTO projects(
                    root, indexed_at, file_count, symbol_count, include_count,
                    diagnostics_json, mode, confidence
                 ) VALUES('D:/legacy-copy', 'then', 0, 1, 0, '[]', 'heuristic', 'heuristic')",
                [],
            )
            .unwrap();
        let source_project_id: i64 = source
            .query_row("SELECT id FROM projects", [], |row| row.get(0))
            .unwrap();
        source
            .execute(
                "INSERT INTO symbols(
                    project_id, file, line, kind, name, signature, scope,
                    is_definition, confidence
                 ) VALUES(?1, 'src/legacy.cpp', 4, 'function', 'legacyRun',
                    'void legacyRun()', '', 1, 'heuristic')",
                rusqlite::params![source_project_id],
            )
            .unwrap();
        let destination = open_memory_db();
        let destination_project_id =
            super::copy_project_between_dbs(&source, &destination, source_project_id).unwrap();

        let stable_key: String = destination
            .query_row(
                "SELECT stable_key FROM symbols WHERE project_id = ?1",
                rusqlite::params![destination_project_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(stable_key.starts_with("sym:"));
        let status = crate::index::load_index_status(&destination, destination_project_id).unwrap();
        assert_eq!(status.schema_version, crate::index::INDEX_SCHEMA_VERSION);
        assert_eq!(status.generator_version, "legacy-db");
        assert_eq!(status.generation, 1);
        assert!(status.complete);
    }

    #[test]
    fn project_copy_preserves_schema_three_symbol_and_call_ownership() {
        let source = open_memory_db();
        source
            .execute(
                "INSERT INTO projects(
                    root, indexed_at, file_count, symbol_count, include_count,
                    diagnostics_json, mode, confidence, index_schema_version,
                    generator_version, index_generation, index_complete
                 ) VALUES('D:/rich-copy', 'now', 0, 1, 0, '[]', 'hybrid', 'hybrid',
                    3, 'cppinsight-core/0.1.0', 4, 1)",
                [],
            )
            .unwrap();
        let source_project_id: i64 = source
            .query_row("SELECT id FROM projects", [], |row| row.get(0))
            .unwrap();
        source
            .execute(
                "INSERT INTO symbols(
                    project_id, file, line, kind, name, qualified_name, stable_key,
                    parent_qualified_name, parent_stable_key, signature, scope,
                    is_definition, confidence
                 ) VALUES(?1, 'src/run.cpp', 7, 'function', 'run', 'app::Runner::run',
                    'sym:aaaaaaaaaaaaaaaa', 'app::Runner', 'sym:bbbbbbbbbbbbbbbb',
                    'void Runner::run()', 'app', 1, 'hybrid')",
                rusqlite::params![source_project_id],
            )
            .unwrap();
        source
            .execute(
                "INSERT INTO calls(
                    project_id, file, line, name, target_qualified_name,
                    caller_qualified_name, caller_stable_key, resolved_target_key,
                    candidate_target_keys_json, confidence
                 ) VALUES(?1, 'src/run.cpp', 9, 'helper', 'app::helper',
                    'app::Runner::run', 'sym:aaaaaaaaaaaaaaaa',
                    'sym:cccccccccccccccc', '[\"sym:cccccccccccccccc\"]', 'hybrid')",
                rusqlite::params![source_project_id],
            )
            .unwrap();
        source
            .execute(
                "INSERT INTO symbol_relations(
                    project_id, source_key, target_key, target_name, relation_kind,
                    evidence_file, evidence_line, confidence, ambiguous,
                    candidate_target_keys_json
                 ) VALUES(?1, 'sym:aaaaaaaaaaaaaaaa', 'sym:cccccccccccccccc',
                    'app::helper', 'calls', 'src/run.cpp', 9, 'hybrid', 0,
                    '[\"sym:cccccccccccccccc\"]')",
                rusqlite::params![source_project_id],
            )
            .unwrap();
        source
            .execute(
                "INSERT INTO module_relations(
                    project_id, source_module, target_module, relation_kind,
                    weight, confidence, evidence_count
                 ) VALUES(?1, 'module:src', 'module:lib', 'calls', 2, 'hybrid', 2)",
                rusqlite::params![source_project_id],
            )
            .unwrap();
        source
            .execute(
                "INSERT INTO entry_points(
                    project_id, symbol_key, qualified_name, entry_kind,
                    evidence_file, evidence_line, confidence
                 ) VALUES(?1, 'sym:aaaaaaaaaaaaaaaa', 'app::Runner::run', 'test',
                    'src/run.cpp', 7, 'hybrid')",
                rusqlite::params![source_project_id],
            )
            .unwrap();
        source
            .execute(
                "INSERT INTO graph_layouts(
                    project_id, graph_kind, view_key, protocol_version,
                    positions_json, updated_at
                 ) VALUES(?1, 'function_calls', 'run-view', 1,
                    '{\"sym:aaaaaaaaaaaaaaaa\":{\"x\":1.0,\"y\":2.0}}', 'now')",
                rusqlite::params![source_project_id],
            )
            .unwrap();

        let destination = open_memory_db();
        let destination_project_id =
            super::copy_project_between_dbs(&source, &destination, source_project_id).unwrap();

        let symbol: (String, String, String, String) = destination
            .query_row(
                "SELECT stable_key, qualified_name, parent_stable_key, parent_qualified_name
                 FROM symbols WHERE project_id = ?1",
                rusqlite::params![destination_project_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            symbol,
            (
                "sym:aaaaaaaaaaaaaaaa".into(),
                "app::Runner::run".into(),
                "sym:bbbbbbbbbbbbbbbb".into(),
                "app::Runner".into()
            )
        );
        let call: (String, String, String, String, String) = destination
            .query_row(
                "SELECT caller_stable_key, caller_qualified_name, target_qualified_name,
                        resolved_target_key, candidate_target_keys_json
                 FROM calls WHERE project_id = ?1",
                rusqlite::params![destination_project_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            call,
            (
                "sym:aaaaaaaaaaaaaaaa".into(),
                "app::Runner::run".into(),
                "app::helper".into(),
                "sym:cccccccccccccccc".into(),
                "[\"sym:cccccccccccccccc\"]".into()
            )
        );
        for table in [
            "symbol_relations",
            "module_relations",
            "entry_points",
            "graph_layouts",
        ] {
            let count: i64 = destination
                .query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE project_id = ?1"),
                    rusqlite::params![destination_project_id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "missing copied rows from {table}");
        }
    }

    #[test]
    fn graph_retrieval_prefers_definition_and_call_site_chunks() {
        let connection = open_memory_db();
        connection
            .execute(
                "INSERT INTO projects(root, indexed_at, file_count, symbol_count, include_count, diagnostics_json, mode, confidence)
                 VALUES('D:/demo', 'now', 2, 2, 0, '[]', 'heuristic', 'heuristic')",
                [],
            )
            .unwrap();
        let project_id: i64 = connection
            .query_row("SELECT id FROM projects", [], |row| row.get(0))
            .unwrap();

        connection
            .execute(
                "INSERT INTO symbols(project_id, file, line, kind, name, signature, scope, is_definition, confidence)
                 VALUES(?1, 'src/crypto.cpp', 12, 'function', 'encryptFile', 'void encryptFile()', '', 1, 'heuristic')",
                rusqlite::params![project_id],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO symbols(project_id, file, line, kind, name, signature, scope, is_definition, confidence)
                 VALUES(?1, 'src/main.cpp', 30, 'function', 'runPipeline', 'void runPipeline()', '', 1, 'heuristic')",
                rusqlite::params![project_id],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO calls(project_id, file, line, name, confidence)
                 VALUES(?1, 'src/main.cpp', 40, 'encryptFile', 'heuristic')",
                rusqlite::params![project_id],
            )
            .unwrap();

        // Unrelated chunk with keyword noise that would win pure text scoring.
        connection
            .execute(
                "INSERT INTO chunks(project_id, file, start_line, end_line, text)
                 VALUES(?1, 'docs/notes.txt', 1, 20, 'encryptFile encryptFile encryptFile noise')",
                rusqlite::params![project_id],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO chunks(project_id, file, start_line, end_line, text)
                 VALUES(?1, 'src/crypto.cpp', 1, 40, 'void encryptFile() { /* def */ }')",
                rusqlite::params![project_id],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO chunks(project_id, file, start_line, end_line, text)
                 VALUES(?1, 'src/main.cpp', 21, 60, 'void runPipeline() { encryptFile(); }')",
                rusqlite::params![project_id],
            )
            .unwrap();

        let selected =
            super::search_contexts(&connection, project_id, "Where is encryptFile used?", 3)
                .expect("search");
        let files: HashSet<_> = selected.iter().map(|item| item.file.as_str()).collect();
        assert!(
            files.contains("src/crypto.cpp"),
            "expected definition chunk, got {:?}",
            selected.iter().map(|c| &c.file).collect::<Vec<_>>()
        );
        assert!(
            files.contains("src/main.cpp"),
            "expected call-site/caller chunk, got {:?}",
            selected.iter().map(|c| &c.file).collect::<Vec<_>>()
        );
        assert!(selected[0].score >= selected.iter().map(|c| c.score).max().unwrap_or(0));
        assert_ne!(selected[0].file, "docs/notes.txt");
    }

    #[test]
    fn inspect_symbol_reports_callers_callees_and_entry_path() {
        let connection = open_memory_db();
        connection
            .execute(
                "INSERT INTO projects(root, indexed_at, file_count, symbol_count, include_count, diagnostics_json, mode, confidence)
                 VALUES('D:/demo', 'now', 2, 3, 0, '[]', 'heuristic', 'heuristic')",
                [],
            )
            .unwrap();
        let project_id: i64 = connection
            .query_row("SELECT id FROM projects", [], |row| row.get(0))
            .unwrap();

        for (file, line, name) in [
            ("src/main.cpp", 1_i64, "main"),
            ("src/main.cpp", 20, "runPipeline"),
            ("src/crypto.cpp", 10, "encryptFile"),
        ] {
            connection
                .execute(
                    "INSERT INTO symbols(project_id, file, line, kind, name, signature, scope, is_definition, confidence)
                     VALUES(?1, ?2, ?3, 'function', ?4, '', '', 1, 'heuristic')",
                    rusqlite::params![project_id, file, line, name],
                )
                .unwrap();
        }
        connection
            .execute(
                "INSERT INTO calls(project_id, file, line, name, confidence)
                 VALUES(?1, 'src/main.cpp', 25, 'encryptFile', 'heuristic')",
                rusqlite::params![project_id],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO calls(project_id, file, line, name, confidence)
                 VALUES(?1, 'src/main.cpp', 5, 'runPipeline', 'heuristic')",
                rusqlite::params![project_id],
            )
            .unwrap();

        let response =
            super::inspect_symbol_at(&connection, project_id, "encryptFile", Some(2)).unwrap();
        let report = response.report.expect("report");
        assert_eq!(report.symbol, "encryptFile");
        assert!(report.callers.iter().any(|item| item.name == "runPipeline"));
        assert!(report.entry_paths.iter().any(|path| path.entry == "main"));
        assert!(report
            .entry_paths
            .iter()
            .any(|path| path.hops.iter().any(|hop| hop.name == "encryptFile")));
    }
}
