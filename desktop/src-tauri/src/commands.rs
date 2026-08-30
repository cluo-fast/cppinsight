use crate::diagnostics::{build_report, write_report, DiagnosticProjectInput, DiagnosticReport};
use crate::graph::{
    query_graph as query_graph_at, GraphKind, GraphQueryRequest, GraphQueryResponse,
    SavedGraphLayout,
};
use crate::index::{
    load_graph_layout as load_graph_layout_at, save_graph_layout as save_graph_layout_at,
};
use crate::{
    app_data_dir_from_app,
    workspace::{
        load_chats_for_root_key, load_workspace_snapshot, save_workspace_snapshot,
        workspace_db_path, PersistedChat, WorkspaceSnapshot,
    },
};
use crate::{
    ask_project_at_with_settings, build_ask_context_preview, build_client_summary,
    emit_index_progress, index_db_path_for_root, index_scan_json_at, inspect_symbol_at,
    load_project_overview, migrate_legacy_api_keys, migrate_legacy_project_if_needed,
    open_database_at, overview_from_scan, project_index_db_path, public_openai_settings,
    read_openai_settings, run_scan, set_openai_settings_at, settings_db_path_from_app,
    try_load_indexed_project_at, write_index, AskResponse, IndexProgressEvent,
    InspectSymbolResponse, ProjectOverview, PublicOpenAiSettings, ScanResult, UpdateOpenAiSettings,
};
use rusqlite::Connection;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, State};

const SAMPLE_PROJECT_MARKER: &str = ".cppinsight-sample-v2";

#[derive(Default)]
pub struct IndexCancellation {
    active: Mutex<Option<Arc<AtomicBool>>>,
}

impl IndexCancellation {
    fn begin(&self) -> Result<Arc<AtomicBool>, String> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| "index cancellation state is unavailable".to_string())?;
        if active.is_some() {
            return Err("an index operation is already running".to_string());
        }
        let token = Arc::new(AtomicBool::new(false));
        *active = Some(token.clone());
        Ok(token)
    }

    fn cancel(&self) -> bool {
        let Ok(active) = self.active.lock() else {
            return false;
        };
        if let Some(token) = active.as_ref() {
            token.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    fn finish(&self, token: &Arc<AtomicBool>) {
        if let Ok(mut active) = self.active.lock() {
            if active
                .as_ref()
                .map(|current| Arc::ptr_eq(current, token))
                .unwrap_or(false)
            {
                *active = None;
            }
        }
    }
}
const SAMPLE_PROJECT_FILES: &[(&str, &str)] = &[
    (
        "CMakeLists.txt",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/sample-cpp/CMakeLists.txt"
        )),
    ),
    (
        "compile_commands.json",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/sample-cpp/compile_commands.json"
        )),
    ),
    (
        "include/sample/engine.hpp",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/sample-cpp/include/sample/engine.hpp"
        )),
    ),
    (
        "src/engine.cpp",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/sample-cpp/src/engine.cpp"
        )),
    ),
    (
        "include/sample/worker.hpp",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/sample-cpp/include/sample/worker.hpp"
        )),
    ),
    (
        "src/worker.cpp",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/sample-cpp/src/worker.cpp"
        )),
    ),
    (
        "src/main.cpp",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/sample-cpp/src/main.cpp"
        )),
    ),
];

fn materialize_sample_project(root: &Path) -> Result<(), String> {
    fs::create_dir_all(root).map_err(|error| format!("create sample project failed: {error}"))?;
    let marker = root.join(SAMPLE_PROJECT_MARKER);
    if marker.exists() {
        return Ok(());
    }
    for (relative, content) in SAMPLE_PROJECT_FILES {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("create sample project directory failed: {error}"))?;
        }
        fs::write(path, content)
            .map_err(|error| format!("write sample project file failed: {error}"))?;
    }
    fs::write(marker, "CppInsight AI bundled sample project v2\n")
        .map_err(|error| format!("write sample project marker failed: {error}"))?;
    Ok(())
}

#[tauri::command]
pub fn prepare_sample_project(app: AppHandle) -> Result<String, String> {
    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("failed to resolve sample project cache dir: {error}"))?;
    let root = cache_dir.join("sample-project-v2");
    materialize_sample_project(&root)?;
    Ok(root.to_string_lossy().into_owned())
}

fn collect_diagnostic_report(app: &tauri::AppHandle) -> Result<DiagnosticReport, String> {
    let app_data = app_data_dir_from_app(app)?;
    let workspace_path = workspace_db_path(&app_data);
    let snapshot = load_workspace_snapshot(&workspace_path)?;
    let mut projects = Vec::with_capacity(snapshot.open_projects.len());

    for project in &snapshot.open_projects {
        let root = PathBuf::from(&project.root_path);
        let db_path = project_index_db_path(&root);
        let conversation_count = snapshot
            .chats
            .iter()
            .filter(|chat| chat.root_key == project.root_key)
            .count() as u64;
        let mut diagnostic = DiagnosticProjectInput {
            root_path: project.root_path.clone(),
            index_status: "missing".to_string(),
            file_count: 0,
            symbol_count: 0,
            include_count: 0,
            conversation_count,
            mode: "unknown".to_string(),
            confidence: "unknown".to_string(),
            diagnostics: vec!["index_missing".to_string()],
        };

        match try_load_indexed_project_at(&root, &db_path) {
            Ok(Some(overview)) => {
                diagnostic.index_status = "ready".to_string();
                diagnostic.file_count = overview.file_count;
                diagnostic.symbol_count = overview.symbol_count;
                diagnostic.include_count = overview.include_count;
                diagnostic.mode = overview.mode;
                diagnostic.confidence = overview.confidence;
                diagnostic.diagnostics = overview.diagnostics;
            }
            Ok(None) => {}
            Err(_) => {
                diagnostic.index_status = "unavailable".to_string();
                diagnostic.diagnostics = vec!["index_unavailable".to_string()];
            }
        }
        projects.push(diagnostic);
    }

    Ok(build_report(
        &chrono::Utc::now().to_rfc3339(),
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        projects,
    ))
}

fn validate_project_scope(
    connection: &Connection,
    project_id: i64,
    root: &str,
) -> Result<(), String> {
    if project_id <= 0 {
        return Err("project_id must be positive".to_string());
    }
    let stored_root = connection
        .query_row(
            "SELECT root FROM projects WHERE id = ?1",
            [project_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| format!("project scope lookup failed: {error}"))?;
    let expected = PathBuf::from(root)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(root));
    let actual = PathBuf::from(stored_root.clone())
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(stored_root));
    if expected != actual {
        return Err("project_id does not belong to the requested root".to_string());
    }
    Ok(())
}

fn load_project_overview_scoped(
    connection: &Connection,
    project_id: i64,
    root: &str,
) -> Result<ProjectOverview, String> {
    validate_project_scope(connection, project_id, root)?;
    load_project_overview(connection, project_id)
}

#[derive(Debug, Clone, Serialize)]
pub struct LoadedWorkspace {
    pub active_root_key: Option<String>,
    pub projects: Vec<LoadedProject>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoadedProject {
    pub root_key: String,
    pub root_path: String,
    pub index_missing: bool,
    pub overview: Option<ProjectOverview>,
    pub chats: Vec<PersistedChat>,
    pub active_chat_id: Option<String>,
}

#[tauri::command]
pub async fn scan_project(path: String) -> Result<ScanResult, String> {
    tauri::async_runtime::spawn_blocking(move || run_scan(&path))
        .await
        .map_err(|error| format!("scan task join error: {error}"))?
}

#[tauri::command]
pub async fn index_project(
    app: AppHandle,
    path: String,
    cancellation: State<'_, IndexCancellation>,
) -> Result<ProjectOverview, String> {
    let token = cancellation.begin()?;
    let token_for_task = token.clone();
    let app_for_task = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let scan = crate::run_scan_with_progress_cancelable(
            &path,
            |event| {
                emit_index_progress(&app_for_task, &event);
            },
            &token_for_task,
        )?;
        if token_for_task.load(Ordering::Relaxed) {
            return Err("indexing cancelled".to_string());
        }

        emit_index_progress(
            &app_for_task,
            &IndexProgressEvent {
                stage: "indexing".to_string(),
                files_discovered: None,
                files_analyzed: None,
                files_reused: None,
                current_file: None,
                message: Some("正在写入索引…".to_string()),
            },
        );

        let root = PathBuf::from(&scan.root);
        let db_path = index_db_path_for_root(&app_for_task, &root)?;
        // Best-effort migration; ignore soft failures and continue with fresh write.
        if let Err(error) = migrate_legacy_project_if_needed(&app_for_task, &root, &db_path) {
            eprintln!("warning: legacy index migration skipped: {error}");
        }
        if token_for_task.load(Ordering::Relaxed) {
            return Err("indexing cancelled".to_string());
        }

        let mut connection = open_database_at(&db_path)?;
        let project_id = write_index(&mut connection, &scan)?;
        let overview = overview_from_scan(project_id, &scan)?;

        emit_index_progress(
            &app_for_task,
            &IndexProgressEvent {
                stage: "done".to_string(),
                files_discovered: Some(overview.file_count),
                files_analyzed: None,
                files_reused: None,
                current_file: None,
                message: Some("索引完成".to_string()),
            },
        );

        Ok(overview)
    })
    .await
    .map_err(|error| format!("index task join error: {error}"));
    cancellation.finish(&token);
    result?
}

#[tauri::command]
pub fn cancel_index(cancellation: State<'_, IndexCancellation>) -> bool {
    cancellation.cancel()
}

#[tauri::command]
pub fn index_scan_json(
    app: tauri::AppHandle,
    scan_json: String,
) -> Result<ProjectOverview, String> {
    let json_text = std::fs::read_to_string(&scan_json)
        .map_err(|error| format!("failed to read scan JSON {scan_json}: {error}"))?;
    let scan: ScanResult = serde_json::from_str(&json_text)
        .map_err(|error| format!("failed to parse scan JSON: {error}"))?;
    let root = PathBuf::from(&scan.root);
    let db_path = index_db_path_for_root(&app, &root)?;
    if let Err(error) = migrate_legacy_project_if_needed(&app, &root, &db_path) {
        eprintln!("warning: legacy index migration skipped: {error}");
    }
    index_scan_json_at(&scan_json, &db_path)
}

#[tauri::command]
pub fn get_project_overview(
    app: tauri::AppHandle,
    root: String,
    project_id: i64,
) -> Result<ProjectOverview, String> {
    let db_path = index_db_path_for_root(&app, &PathBuf::from(&root))?;
    let connection = open_database_at(&db_path)?;
    load_project_overview_scoped(&connection, project_id, &root)
}

/// Open an already-indexed project without rescanning. Returns `null` if no local index exists.
#[tauri::command]
pub fn load_indexed_project(
    app: tauri::AppHandle,
    path: String,
) -> Result<Option<ProjectOverview>, String> {
    let root = PathBuf::from(&path);
    let db_path = index_db_path_for_root(&app, &root)?;
    if let Err(error) = migrate_legacy_project_if_needed(&app, &root, &db_path) {
        eprintln!("warning: legacy index migration skipped: {error}");
    }
    try_load_indexed_project_at(&root, &db_path)
}

#[tauri::command]
pub fn load_workspace(app: tauri::AppHandle) -> Result<LoadedWorkspace, String> {
    if let Err(error) = migrate_legacy_api_keys(&app) {
        eprintln!("warning: legacy API key migration deferred: {error}");
    }
    let app_data = app_data_dir_from_app(&app)?;
    let db = workspace_db_path(&app_data);
    let snap = load_workspace_snapshot(&db)?;

    let mut projects = Vec::new();
    for open in snap.open_projects {
        let root = PathBuf::from(&open.root_path);
        let index_db = project_index_db_path(&root);
        let overview = try_load_indexed_project_at(&root, &index_db)?;
        let index_missing = overview.is_none();
        let chats: Vec<_> = snap
            .chats
            .iter()
            .filter(|c| c.root_key == open.root_key)
            .cloned()
            .collect();
        let active_chat_id = chats
            .iter()
            .find(|c| c.active)
            .map(|c| c.id.clone())
            .or_else(|| chats.first().map(|c| c.id.clone()));
        projects.push(LoadedProject {
            root_key: open.root_key,
            root_path: open.root_path,
            index_missing,
            overview,
            chats,
            active_chat_id,
        });
    }

    Ok(LoadedWorkspace {
        active_root_key: snap.active_root_key,
        projects,
    })
}

#[tauri::command]
pub fn save_workspace(app: tauri::AppHandle, snapshot: WorkspaceSnapshot) -> Result<(), String> {
    let app_data = app_data_dir_from_app(&app)?;
    let db = workspace_db_path(&app_data);
    save_workspace_snapshot(&db, &snapshot)
}

#[tauri::command]
pub fn load_project_chats(
    app: tauri::AppHandle,
    root_key: String,
) -> Result<Vec<PersistedChat>, String> {
    let app_data = app_data_dir_from_app(&app)?;
    let db = workspace_db_path(&app_data);
    load_chats_for_root_key(&db, &root_key)
}

#[tauri::command]
pub fn get_openai_settings(app: tauri::AppHandle) -> Result<PublicOpenAiSettings, String> {
    migrate_legacy_api_keys(&app)?;
    let settings_path = settings_db_path_from_app(&app)?;
    let connection = open_database_at(&settings_path)?;
    read_openai_settings(&connection).map(|settings| public_openai_settings(&settings))
}

#[tauri::command]
pub fn set_openai_settings(
    app: tauri::AppHandle,
    settings: UpdateOpenAiSettings,
) -> Result<(), String> {
    migrate_legacy_api_keys(&app)?;
    let settings_path = settings_db_path_from_app(&app)?;
    set_openai_settings_at(&settings_path, &settings)
}

#[tauri::command]
pub fn preview_diagnostics(app: tauri::AppHandle) -> Result<DiagnosticReport, String> {
    collect_diagnostic_report(&app)
}

#[tauri::command]
pub fn export_diagnostics(app: tauri::AppHandle, path: String) -> Result<String, String> {
    let target = PathBuf::from(path);
    let report = collect_diagnostic_report(&app)?;
    write_report(&target, &report)?;
    Ok(target.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn ask_project(
    app: tauri::AppHandle,
    root: String,
    project_id: i64,
    question: String,
) -> Result<AskResponse, String> {
    migrate_legacy_api_keys(&app)?;
    let db_path = index_db_path_for_root(&app, &PathBuf::from(&root))?;
    let expected_root = root.clone();
    let validation_db_path = db_path.clone();
    let settings_path = settings_db_path_from_app(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let connection = open_database_at(&validation_db_path)?;
        validate_project_scope(&connection, project_id, &expected_root)
    })
    .await
    .map_err(|error| format!("ask scope validation join error: {error}"))??;
    ask_project_at_with_settings(project_id, &question, &db_path, Some(&settings_path)).await
}

#[tauri::command]
pub async fn preview_ask_context(
    app: tauri::AppHandle,
    root: String,
    project_id: i64,
    question: String,
) -> Result<Vec<crate::Citation>, String> {
    let db_path = index_db_path_for_root(&app, Path::new(&root))?;
    let expected_root = root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = open_database_at(&db_path)?;
        validate_project_scope(&connection, project_id, &expected_root)?;
        build_ask_context_preview(&connection, project_id, &question)
    })
    .await
    .map_err(|error| format!("ask context preview join error: {error}"))?
}

#[tauri::command]
pub async fn export_client_summary(
    app: tauri::AppHandle,
    root: String,
    project_id: i64,
) -> Result<String, String> {
    let db_path = index_db_path_for_root(&app, &PathBuf::from(&root))?;
    let expected_root = root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = open_database_at(&db_path)?;
        validate_project_scope(&connection, project_id, &expected_root)?;
        build_client_summary(&connection, project_id)
    })
    .await
    .map_err(|error| format!("export summary join error: {error}"))?
}

#[tauri::command]
pub async fn inspect_symbol(
    app: tauri::AppHandle,
    root: String,
    project_id: i64,
    query: String,
    depth: Option<u32>,
) -> Result<InspectSymbolResponse, String> {
    let db_path = index_db_path_for_root(&app, &PathBuf::from(&root))?;
    let expected_root = root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = open_database_at(&db_path)?;
        validate_project_scope(&connection, project_id, &expected_root)?;
        inspect_symbol_at(&connection, project_id, &query, depth)
    })
    .await
    .map_err(|error| format!("inspect task join error: {error}"))?
}

#[tauri::command]
pub async fn query_graph(
    app: tauri::AppHandle,
    root: String,
    request: GraphQueryRequest,
) -> Result<GraphQueryResponse, String> {
    let db_path = index_db_path_for_root(&app, &PathBuf::from(&root))?;
    let expected_root = root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = open_database_at(&db_path)?;
        validate_project_scope(&connection, request.project_id, &expected_root)?;
        query_graph_at(&connection, request).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("graph query task join error: {error}"))?
}

#[tauri::command]
pub async fn save_graph_layout(
    app: tauri::AppHandle,
    root: String,
    layout: SavedGraphLayout,
) -> Result<(), String> {
    let db_path = index_db_path_for_root(&app, &PathBuf::from(&root))?;
    let expected_root = root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = open_database_at(&db_path)?;
        validate_project_scope(&connection, layout.project_id, &expected_root)?;
        save_graph_layout_at(&connection, layout).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("save graph layout task join error: {error}"))?
}

#[tauri::command]
pub async fn load_graph_layout(
    app: tauri::AppHandle,
    root: String,
    project_id: i64,
    graph_kind: GraphKind,
    view_key: String,
) -> Result<Option<SavedGraphLayout>, String> {
    let db_path = index_db_path_for_root(&app, &PathBuf::from(&root))?;
    let expected_root = root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = open_database_at(&db_path)?;
        validate_project_scope(&connection, project_id, &expected_root)?;
        load_graph_layout_at(&connection, project_id, graph_kind, &view_key)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("load graph layout task join error: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::{
        load_project_overview_scoped, materialize_sample_project, validate_project_scope,
        IndexCancellation,
    };
    use std::sync::atomic::Ordering;

    #[test]
    fn bundled_sample_project_is_complete_and_idempotent() {
        let directory = tempfile::tempdir().expect("temp directory");
        materialize_sample_project(directory.path()).expect("materialize sample");
        assert!(directory.path().join("CMakeLists.txt").is_file());
        assert!(directory.path().join("compile_commands.json").is_file());
        assert!(directory.path().join("src/main.cpp").is_file());
        assert!(directory.path().join("include/sample/engine.hpp").is_file());
        let custom = directory.path().join("src/main.cpp");
        std::fs::write(&custom, "custom\n").expect("write custom file");
        materialize_sample_project(directory.path()).expect("reuse sample");
        assert_eq!(
            std::fs::read_to_string(custom).expect("read custom"),
            "custom\n"
        );
    }

    #[test]
    fn project_scope_validation_rejects_cross_root_and_invalid_ids() {
        let directory = tempfile::tempdir().expect("temp directory");
        let connection = rusqlite::Connection::open_in_memory().expect("open database");
        crate::migrate_database(&connection).expect("migrate database");
        connection
            .execute(
                "INSERT INTO projects(
                    root, indexed_at, file_count, symbol_count, include_count,
                    diagnostics_json, mode, confidence, index_schema_version,
                    generator_version, index_generation, index_complete
                 ) VALUES(?1, 'now', 0, 0, 0, '[]', 'heuristic', 'heuristic', 3, 'test', 1, 1)",
                rusqlite::params![directory.path().to_string_lossy().to_string()],
            )
            .expect("insert project");

        validate_project_scope(&connection, 1, &directory.path().to_string_lossy())
            .expect("matching root should pass");
        assert!(validate_project_scope(&connection, 1, "different-root").is_err());
        assert!(
            validate_project_scope(&connection, 0, directory.path().to_str().unwrap()).is_err()
        );
    }

    #[test]
    fn project_overview_loader_rejects_cross_scope() {
        let directory = tempfile::tempdir().expect("temp directory");
        let connection = rusqlite::Connection::open_in_memory().expect("open database");
        crate::migrate_database(&connection).expect("migrate database");
        connection
            .execute(
                "INSERT INTO projects(
                    root, indexed_at, file_count, symbol_count, include_count,
                    diagnostics_json, mode, confidence, index_schema_version,
                    generator_version, index_generation, index_complete
                 ) VALUES(?1, 'now', 0, 0, 0, '[]', 'heuristic', 'heuristic', 3, 'test', 1, 1)",
                rusqlite::params![directory.path().to_string_lossy().to_string()],
            )
            .expect("insert project");

        assert!(load_project_overview_scoped(&connection, 1, "different-root").is_err());
    }

    #[test]
    fn index_cancellation_registry_targets_only_the_active_job() {
        let registry = IndexCancellation::default();
        assert!(!registry.cancel());
        let token = registry.begin().expect("first job should register");
        assert!(
            registry.begin().is_err(),
            "second job must not replace token"
        );
        assert!(registry.cancel());
        assert!(token.load(Ordering::Relaxed));
        registry.finish(&token);
        assert!(!registry.cancel());
    }
}
