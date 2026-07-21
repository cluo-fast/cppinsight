use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::Manager;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct LineStats {
    total: u64,
    code: u64,
    comment: u64,
    blank: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ScanStats {
    source_files: u64,
    header_files: u64,
    inline_files: u64,
    total_bytes: u64,
    lines: LineStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IncludeInfo {
    file: String,
    line: u64,
    target: String,
    kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SymbolInfo {
    file: String,
    line: u64,
    kind: String,
    name: String,
    signature: String,
    scope: String,
    is_definition: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CallInfo {
    file: String,
    line: u64,
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileInfo {
    path: String,
    extension: String,
    category: String,
    size_bytes: u64,
    lines: LineStats,
    #[serde(default)]
    includes: Vec<IncludeInfo>,
    #[serde(default)]
    symbols: Vec<SymbolInfo>,
    #[serde(default)]
    calls: Vec<CallInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScanResult {
    #[serde(default)]
    schema_version: u32,
    root: String,
    file_count: usize,
    skipped_entries: u64,
    stats: ScanStats,
    #[serde(default)]
    extensions: BTreeMap<String, u64>,
    #[serde(default)]
    top_level_layout: BTreeMap<String, u64>,
    #[serde(default)]
    diagnostics: Vec<String>,
    #[serde(default)]
    includes: Vec<IncludeInfo>,
    #[serde(default)]
    symbols: Vec<SymbolInfo>,
    #[serde(default)]
    calls: Vec<CallInfo>,
    #[serde(default)]
    files: Vec<FileInfo>,
}

#[derive(Debug, Clone, Serialize)]
struct HotFile {
    path: String,
    total_lines: u64,
    code_lines: u64,
    symbol_count: u64,
}

#[derive(Debug, Clone, Serialize)]
struct ProjectOverview {
    project_id: i64,
    root: String,
    indexed_at: String,
    file_count: u64,
    symbol_count: u64,
    include_count: u64,
    stats: ScanStats,
    extensions: BTreeMap<String, u64>,
    top_level_layout: BTreeMap<String, u64>,
    hotspots: Vec<HotFile>,
    symbols: Vec<SymbolInfo>,
    diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct Citation {
    file: String,
    line: u64,
    title: String,
}

#[derive(Debug, Clone, Serialize)]
struct AskResponse {
    answer: String,
    citations: Vec<Citation>,
}

#[derive(Debug, Clone)]
struct SearchContext {
    file: String,
    start_line: u64,
    end_line: u64,
    text: String,
    score: i64,
}

#[tauri::command]
fn scan_project(path: String) -> Result<ScanResult, String> {
    run_scan(&path)
}

#[tauri::command]
fn index_project(app: tauri::AppHandle, path: String) -> Result<ProjectOverview, String> {
    let scan = run_scan(&path)?;
    let mut connection = open_database(&app)?;
    let project_id = write_index(&mut connection, &scan)?;
    overview_from_scan(project_id, &scan)
}

#[tauri::command]
fn get_project_overview(app: tauri::AppHandle, project_id: i64) -> Result<ProjectOverview, String> {
    let connection = open_database(&app)?;
    load_project_overview(&connection, project_id)
}

#[tauri::command]
fn set_openai_api_key(app: tauri::AppHandle, api_key: String) -> Result<(), String> {
    let connection = open_database(&app)?;
    connection
        .execute(
            "INSERT INTO settings(key, value) VALUES('openai_api_key', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![api_key.trim()],
        )
        .map_err(sql_error)?;
    Ok(())
}

#[tauri::command]
async fn ask_project(
    app: tauri::AppHandle,
    project_id: i64,
    question: String,
) -> Result<AskResponse, String> {
    let (contexts, api_key) = {
        let connection = open_database(&app)?;
        let contexts = search_contexts(&connection, project_id, &question, 8)?;
        let api_key = read_openai_api_key(&connection)?;
        (contexts, api_key)
    };

    if contexts.is_empty() {
        return Ok(AskResponse {
            answer: "I could not find indexed project context for that question. Re-index the project and try again.".to_string(),
            citations: Vec::new(),
        });
    }

    let Some(api_key) = api_key else {
        return Ok(AskResponse {
            answer: "OpenAI API key is not configured. Set OPENAI_API_KEY before launching, or save a key in the settings panel.".to_string(),
            citations: contexts_to_citations(&contexts),
        });
    };

    let answer = call_openai(&api_key, &question, &contexts).await?;
    Ok(AskResponse {
        answer,
        citations: contexts_to_citations(&contexts),
    })
}

fn run_scan(path: &str) -> Result<ScanResult, String> {
    let cli = find_cppinsight_binary()?;
    let json_path = std::env::temp_dir().join(format!(
        "cppinsight-scan-{}-{}.json",
        std::process::id(),
        Utc::now().timestamp_millis()
    ));

    let output = Command::new(&cli)
        .arg("scan")
        .arg(path)
        .arg("--json")
        .arg(&json_path)
        .output()
        .map_err(|error| format!("failed to run cppinsight CLI: {error}"))?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "cppinsight scan failed with status {}.\n{}\n{}",
            output.status, stdout, stderr
        ));
    }

    let json_text = fs::read_to_string(&json_path)
        .map_err(|error| format!("failed to read scan JSON {}: {error}", json_path.display()))?;
    let _ = fs::remove_file(&json_path);

    serde_json::from_str(&json_text).map_err(|error| format!("failed to parse scan JSON: {error}"))
}

fn find_cppinsight_binary() -> Result<PathBuf, String> {
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
            candidates.push(parent.join("..").join("cppinsight.exe"));
            candidates.push(parent.join("..").join("..").join("build-codex").join("Release").join("cppinsight.exe"));
            candidates.push(parent.join("..").join("..").join("build").join("Release").join("cppinsight.exe"));
        }
    }

    candidates.push(PathBuf::from("../build-codex/Release/cppinsight.exe"));
    candidates.push(PathBuf::from("../build-codex/Debug/cppinsight.exe"));
    candidates.push(PathBuf::from("../build/Release/cppinsight.exe"));
    candidates.push(PathBuf::from("../build/Debug/cppinsight.exe"));

    for candidate in candidates {
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    Err("cppinsight CLI was not found. Build it first or set CPPINSIGHT_BIN to the executable path.".to_string())
}

fn open_database(app: &tauri::AppHandle) -> Result<Connection, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve app data dir: {error}"))?;
    fs::create_dir_all(&data_dir)
        .map_err(|error| format!("failed to create app data dir {}: {error}", data_dir.display()))?;

    let connection = Connection::open(data_dir.join("projects.sqlite")).map_err(sql_error)?;
    migrate_database(&connection)?;
    Ok(connection)
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
                diagnostics_json TEXT NOT NULL
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
                FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS includes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_id INTEGER NOT NULL,
                file TEXT NOT NULL,
                line INTEGER NOT NULL,
                target TEXT NOT NULL,
                kind TEXT NOT NULL,
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
    Ok(())
}

fn write_index(connection: &mut Connection, scan: &ScanResult) -> Result<i64, String> {
    let transaction = connection.transaction().map_err(sql_error)?;
    let indexed_at = Utc::now().to_rfc3339();
    let diagnostics_json = serde_json::to_string(&scan.diagnostics)
        .map_err(|error| format!("failed to encode diagnostics: {error}"))?;

    transaction
        .execute(
            "INSERT INTO projects(root, indexed_at, file_count, symbol_count, include_count, diagnostics_json)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(root) DO UPDATE SET
                indexed_at = excluded.indexed_at,
                file_count = excluded.file_count,
                symbol_count = excluded.symbol_count,
                include_count = excluded.include_count,
                diagnostics_json = excluded.diagnostics_json",
            params![
                scan.root,
                indexed_at,
                scan.files.len() as i64,
                scan.symbols.len() as i64,
                scan.includes.len() as i64,
                diagnostics_json
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
                "INSERT INTO files(project_id, path, extension, category, size_bytes, total_lines, code_lines, comment_lines, blank_lines)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
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
                ],
            )
            .map_err(sql_error)?;
    }

    for symbol in &scan.symbols {
        transaction
            .execute(
                "INSERT INTO symbols(project_id, file, line, kind, name, signature, scope, is_definition)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    project_id,
                    symbol.file,
                    symbol.line as i64,
                    symbol.kind,
                    symbol.name,
                    symbol.signature,
                    symbol.scope,
                    if symbol.is_definition { 1 } else { 0 },
                ],
            )
            .map_err(sql_error)?;
    }

    for include in &scan.includes {
        transaction
            .execute(
                "INSERT INTO includes(project_id, file, line, target, kind)
                 VALUES(?1, ?2, ?3, ?4, ?5)",
                params![project_id, include.file, include.line as i64, include.target, include.kind],
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

fn overview_from_scan(project_id: i64, scan: &ScanResult) -> Result<ProjectOverview, String> {
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
        stats: scan.stats.clone(),
        extensions: scan.extensions.clone(),
        top_level_layout: scan.top_level_layout.clone(),
        hotspots,
        symbols,
        diagnostics: scan.diagnostics.clone(),
    })
}

fn load_project_overview(connection: &Connection, project_id: i64) -> Result<ProjectOverview, String> {
    let (root, indexed_at, file_count, symbol_count, include_count, diagnostics_json): (
        String,
        String,
        u64,
        u64,
        u64,
        String,
    ) = connection
        .query_row(
            "SELECT root, indexed_at, file_count, symbol_count, include_count, diagnostics_json
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
                ))
            },
        )
        .map_err(sql_error)?;

    let diagnostics = serde_json::from_str(&diagnostics_json).unwrap_or_default();
    let stats = load_stats(connection, project_id)?;
    let extensions = load_group_counts(connection, project_id, "extension")?;
    let top_level_layout = load_top_level_layout(connection, project_id)?;
    let hotspots = load_hotspots(connection, project_id)?;
    let symbols = load_symbols(connection, project_id, 200)?;

    Ok(ProjectOverview {
        project_id,
        root,
        indexed_at,
        file_count,
        symbol_count,
        include_count,
        stats,
        extensions,
        top_level_layout,
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
            "SELECT file, line, kind, name, signature, scope, is_definition
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

fn search_contexts(
    connection: &Connection,
    project_id: i64,
    question: &str,
    limit: usize,
) -> Result<Vec<SearchContext>, String> {
    let tokens = tokenize(question);
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
        context.score = score_context(&context, &tokens);
        if context.score > 0 || contexts.len() < limit {
            contexts.push(context);
        }
    }

    contexts.sort_by(|left, right| right.score.cmp(&left.score).then(left.file.cmp(&right.file)));
    contexts.truncate(limit);
    Ok(contexts)
}

fn tokenize(question: &str) -> Vec<String> {
    let mut tokens = HashSet::new();
    for raw in question.split(|character: char| !character.is_alphanumeric() && character != '_') {
        let token = raw.trim().to_lowercase();
        if token.len() >= 3 {
            tokens.insert(token);
        }
    }
    tokens.into_iter().collect()
}

fn score_context(context: &SearchContext, tokens: &[String]) -> i64 {
    let file = context.file.to_lowercase();
    let text = context.text.to_lowercase();
    let mut score = 0_i64;

    for token in tokens {
        if file.contains(token) {
            score += 12;
        }
        score += text.matches(token).count() as i64;
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

fn read_openai_api_key(connection: &Connection) -> Result<Option<String>, String> {
    if let Ok(value) = std::env::var("OPENAI_API_KEY") {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(Some(trimmed));
        }
    }

    let mut statement = connection
        .prepare("SELECT value FROM settings WHERE key = 'openai_api_key'")
        .map_err(sql_error)?;
    let mut rows = statement.query([]).map_err(sql_error)?;
    if let Some(row) = rows.next().map_err(sql_error)? {
        let value: String = row.get(0).map_err(sql_error)?;
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(Some(trimmed));
        }
    }
    Ok(None)
}

async fn call_openai(
    api_key: &str,
    question: &str,
    contexts: &[SearchContext],
) -> Result<String, String> {
    let model = std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());
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
        "model": model,
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

    let response = reqwest::Client::new()
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(api_key)
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

fn sql_error(error: rusqlite::Error) -> String {
    format!("SQLite error: {error}")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            scan_project,
            index_project,
            get_project_overview,
            set_openai_api_key,
            ask_project
        ])
        .run(tauri::generate_context!())
        .expect("error while running CppInsight AI");
}

fn main() {
    run();
}

#[cfg(test)]
mod tests {
    use super::{score_context, tokenize, SearchContext};

    #[test]
    fn scores_file_and_text_matches() {
        let tokens = tokenize("Where is ProjectScanner implemented?");
        let context = SearchContext {
            file: "src/project/ProjectScanner.cpp".to_string(),
            start_line: 1,
            end_line: 12,
            text: "ProjectScanner scans C++ files".to_string(),
            score: 0,
        };

        assert!(score_context(&context, &tokens) > 0);
    }
}
