mod commands;

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, HashSet};
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    #[serde(default)]
    pub schema_version: u32,
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

#[derive(Debug, Clone)]
pub struct SearchContext {
    pub file: String,
    pub start_line: u64,
    pub end_line: u64,
    pub text: String,
    pub score: i64,
}

pub fn index_scan_json_at(scan_json: &str, db_path: &Path) -> Result<ProjectOverview, String> {
    let json_text = fs::read_to_string(scan_json)
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
    let (contexts, ai_settings) = {
        let connection = open_database_at(db_path)?;
        let contexts = search_contexts(&connection, project_id, question, 8)?;
        let ai_settings = if let Some(settings_path) = settings_db_path {
            let settings = open_database_at(settings_path)?;
            read_openai_settings(&settings)?
        } else {
            read_openai_settings(&connection)?
        };
        (contexts, ai_settings)
    };

    if contexts.is_empty() {
        return Ok(AskResponse {
            answer: "I could not find indexed project context for that question. Re-index the project and try again.".to_string(),
            citations: Vec::new(),
        });
    }

    if ai_settings.api_key.trim().is_empty() {
        return Ok(AskResponse {
            answer: "OpenAI API key is not configured. Set it in Settings, or via OPENAI_API_KEY.".to_string(),
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
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))
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
    let (root, indexed_at, file_count, symbol_count, include_count, diagnostics_json, mode, confidence): (
        String,
        String,
        i64,
        i64,
        i64,
        String,
        String,
        String,
    ) = source
        .query_row(
            "SELECT root, indexed_at, file_count, symbol_count, include_count, diagnostics_json, mode, confidence
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
                ))
            },
        )
        .map_err(sql_error)?;

    destination
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
                root,
                indexed_at,
                file_count,
                symbol_count,
                include_count,
                diagnostics_json,
                mode,
                confidence
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
        .execute("DELETE FROM files WHERE project_id = ?1", params![dest_project_id])
        .map_err(sql_error)?;
    destination
        .execute("DELETE FROM symbols WHERE project_id = ?1", params![dest_project_id])
        .map_err(sql_error)?;
    destination
        .execute("DELETE FROM includes WHERE project_id = ?1", params![dest_project_id])
        .map_err(sql_error)?;
    destination
        .execute("DELETE FROM chunks WHERE project_id = ?1", params![dest_project_id])
        .map_err(sql_error)?;

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
            let (path, extension, category, size_bytes, total_lines, code_lines, comment_lines, blank_lines, mtime_unix) =
                row.map_err(sql_error)?;
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
                "SELECT file, line, kind, name, signature, scope, is_definition, confidence
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
                    row.get::<_, i64>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })
            .map_err(sql_error)?;
        for row in rows {
            let (file, line, kind, name, signature, scope, is_definition, confidence) =
                row.map_err(sql_error)?;
            destination
                .execute(
                    "INSERT INTO symbols(project_id, file, line, kind, name, signature, scope, is_definition, confidence)
                     VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        dest_project_id,
                        file,
                        line,
                        kind,
                        name,
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

    {
        let mut statement = source
            .prepare(
                "SELECT file, start_line, end_line, text FROM chunks WHERE project_id = ?1",
            )
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

    Ok(dest_project_id)
}

fn settings_table_api_key(connection: &Connection) -> Result<Option<String>, String> {
    read_setting_value(connection, "openai_api_key")
}

fn migrate_api_key_once(legacy: &Connection, settings_db: &Path) -> Result<(), String> {
    let settings = open_database_at(settings_db)?;
    if settings_table_api_key(&settings)?.is_some() {
        return Ok(());
    }
    let Some(trimmed) = settings_table_api_key(legacy)? else {
        return Ok(());
    };
    settings
        .execute(
            "INSERT INTO settings(key, value) VALUES('openai_api_key', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![trimmed],
        )
        .map_err(sql_error)?;
    Ok(())
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

        let _ = migrate_api_key_once(&legacy, &settings_path);

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

pub fn run_scan_with_progress(
    path: &str,
    mut on_progress: impl FnMut(IndexProgressEvent),
) -> Result<ScanResult, String> {
    let cli = find_cppinsight_binary()?;
    let json_path = std::env::temp_dir().join(format!(
        "cppinsight-scan-{}-{}.json",
        std::process::id(),
        Utc::now().timestamp_millis()
    ));

    let mut child = Command::new(&cli)
        .arg("scan")
        .arg(path)
        .arg("--json")
        .arg(&json_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to run cppinsight CLI: {error}"))?;

    if let Some(stderr) = child.stderr.take() {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            let Ok(line) = line else {
                continue;
            };
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
        }
    }

    let status = child
        .wait()
        .map_err(|error| format!("failed to wait for cppinsight CLI: {error}"))?;

    if !status.success() {
        let _ = fs::remove_file(&json_path);
        return Err(format!("cppinsight scan failed with status {status}."));
    }

    let json_text = fs::read_to_string(&json_path)
        .map_err(|error| format!("failed to read scan JSON {}: {error}", json_path.display()))?;
    let _ = fs::remove_file(&json_path);

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
            ",
        )
        .map_err(sql_error)?;

    let _ = connection.execute_batch(
        "
        ALTER TABLE projects ADD COLUMN mode TEXT NOT NULL DEFAULT 'heuristic';
        ALTER TABLE projects ADD COLUMN confidence TEXT NOT NULL DEFAULT 'heuristic';
        ALTER TABLE files ADD COLUMN mtime_unix INTEGER NOT NULL DEFAULT 0;
        ALTER TABLE symbols ADD COLUMN confidence TEXT NOT NULL DEFAULT 'heuristic';
        ALTER TABLE includes ADD COLUMN resolved INTEGER NOT NULL DEFAULT 0;
        ALTER TABLE includes ADD COLUMN resolved_path TEXT NOT NULL DEFAULT '';
        ",
    );

    Ok(())
}

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
        .query_row("SELECT id FROM projects WHERE root = ?1", params![scan.root], |row| row.get(0))
        .map_err(sql_error)?;

    transaction
        .execute("DELETE FROM files WHERE project_id = ?1", params![project_id])
        .map_err(sql_error)?;
    transaction
        .execute("DELETE FROM symbols WHERE project_id = ?1", params![project_id])
        .map_err(sql_error)?;
    transaction
        .execute("DELETE FROM includes WHERE project_id = ?1", params![project_id])
        .map_err(sql_error)?;
    transaction
        .execute("DELETE FROM chunks WHERE project_id = ?1", params![project_id])
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
        transaction
            .execute(
                "INSERT INTO symbols(project_id, file, line, kind, name, signature, scope, is_definition, confidence)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    project_id,
                    symbol.file,
                    symbol.line as i64,
                    symbol.kind,
                    symbol.name,
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

    transaction.commit().map_err(sql_error)?;
    Ok(project_id)
}

pub(crate) fn overview_from_scan(project_id: i64, scan: &ScanResult) -> Result<ProjectOverview, String> {
    let mut hotspots = scan
        .files
        .iter()
        .map(|file| HotFile {
            path: file.path.clone(),
            total_lines: file.lines.total,
            code_lines: file.lines.code,
            symbol_count: scan.symbols.iter().filter(|symbol| symbol.file == file.path).count() as u64,
        })
        .collect::<Vec<_>>();
    hotspots.sort_by(|left, right| right.code_lines.cmp(&left.code_lines));
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

pub fn load_project_overview(connection: &Connection, project_id: i64) -> Result<ProjectOverview, String> {
    let (root, indexed_at, file_count, symbol_count, include_count, diagnostics_json, mode, confidence): (
        String,
        String,
        u64,
        u64,
        u64,
        String,
        String,
        String,
    ) = connection
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

fn load_top_level_layout(connection: &Connection, project_id: i64) -> Result<BTreeMap<String, u64>, String> {
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

fn load_second_level_layout(connection: &Connection, project_id: i64) -> Result<BTreeMap<String, u64>, String> {
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

fn load_symbols(connection: &Connection, project_id: i64, limit: i64) -> Result<Vec<SymbolInfo>, String> {
    let mut statement = connection
        .prepare(
            "SELECT file, line, kind, name, signature, scope, is_definition, COALESCE(confidence, 'heuristic')
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
                signature: row.get(4)?,
                scope: row.get(5)?,
                is_definition: row.get::<_, i64>(6)? != 0,
                confidence: row.get(7)?,
            })
        })
        .map_err(sql_error)?;

    let mut symbols = Vec::new();
    for row in rows {
        symbols.push(row.map_err(sql_error)?);
    }
    Ok(symbols)
}

fn build_chunks(scan: &ScanResult) -> Result<Vec<SearchContext>, String> {
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

        for (index, line) in BufReader::new(input).lines().enumerate() {
            let line_number = index as u64 + 1;
            let line = line.map_err(|error| format!("failed to read {}: {error}", path.display()))?;
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

pub fn search_contexts(
    connection: &Connection,
    project_id: i64,
    question: &str,
    limit: usize,
) -> Result<Vec<SearchContext>, String> {
    let tokens = tokenize(question);
    let symbol_names = load_symbol_names(connection, project_id)?;

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

    let mut contexts = Vec::new();
    for row in rows {
        let mut context = row.map_err(sql_error)?;
        context.score = score_context(&context, &tokens, &symbol_names);
        if context.score > 0 || contexts.len() < limit {
            contexts.push(context);
        }
    }

    contexts.sort_by(|left, right| right.score.cmp(&left.score).then(left.file.cmp(&right.file)));
    contexts.truncate(limit);
    Ok(contexts)
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

pub fn tokenize(question: &str) -> Vec<String> {
    let mut tokens = HashSet::new();
    for raw in question.split(|character: char| !character.is_alphanumeric() && character != '_') {
        let token = raw.trim().to_lowercase();
        if token.len() >= 3 {
            tokens.insert(token);
        }
    }
    tokens.into_iter().collect()
}

pub fn score_context(context: &SearchContext, tokens: &[String], symbol_names: &HashSet<String>) -> i64 {
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
            title: format!("{}:{}-{}", context.file, context.start_line, context.end_line),
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiSettings {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
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
    let mut settings = OpenAiSettings::default();

    if let Ok(value) = std::env::var("OPENAI_BASE_URL").or_else(|_| std::env::var("OPENAI_API_BASE"))
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
    } else if let Some(value) = read_setting_value(connection, "openai_api_key")? {
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
    upsert_setting_value(&connection, "openai_api_key", api_key.trim())
}

pub fn set_openai_settings_at(db_path: &Path, settings: &OpenAiSettings) -> Result<(), String> {
    let connection = open_database_at(db_path)?;
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
    upsert_setting_value(&connection, "openai_api_key", settings.api_key.trim())?;
    upsert_setting_value(&connection, "openai_model", &model)?;
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
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        })
        .map_err(sql_error)?;

    let mut projects = Vec::new();
    for row in rows {
        projects.push(row.map_err(sql_error)?);
    }
    Ok(projects)
}

pub fn find_project_id_by_root(db_path: &Path, root: &str) -> Result<i64, String> {
    let connection = open_database_at(db_path)?;
    let canonical = std::fs::canonicalize(root)
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|_| root.to_string());

    connection
        .query_row(
            "SELECT id FROM projects WHERE root = ?1 OR root = ?2 ORDER BY id DESC LIMIT 1",
            params![root, canonical],
            |row| row.get(0),
        )
        .map_err(|_| format!("no indexed project found for root: {root}"))
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
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("OpenAI request failed: {error}"))?;

    let status = response.status();
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|error| format!("failed to parse OpenAI response: {error}"))?;

    if !status.is_success() {
        return Err(format!("OpenAI request failed with {status}: {value}"));
    }

    value["choices"][0]["message"]["content"]
        .as_str()
        .map(|content| content.to_string())
        .ok_or_else(|| format!("OpenAI response did not include message content: {value}"))
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
            commands::set_openai_api_key,
            commands::get_openai_settings,
            commands::set_openai_settings,
            commands::ask_project,
            commands::export_client_summary
        ])
        .run(tauri::generate_context!())
        .expect("error while running CppInsight AI");
}

#[cfg(test)]
mod tests {
    use super::{parse_scan_progress_line, score_context, tokenize, SearchContext};
    use std::collections::HashSet;

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
        assert!(parse_scan_progress_line("[scan] files=abc analyzed=1 reused=0 current=x").is_none());
    }

    #[test]
    fn project_index_db_path_is_under_cppinsight_dir() {
        let path = super::project_index_db_path(std::path::Path::new("D:/repos/demo"));
        assert!(path.ends_with(std::path::Path::new(".cppinsight/index.sqlite"))
            || path.ends_with(std::path::Path::new(".cppinsight\\index.sqlite")));
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
}
