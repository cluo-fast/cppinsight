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
    ask_project_at_with_settings, build_client_summary, emit_index_progress,
    index_db_path_for_root, index_scan_json_at, inspect_symbol_at, load_project_overview,
    migrate_legacy_api_keys, migrate_legacy_project_if_needed, open_database_at,
    overview_from_scan, project_index_db_path, public_openai_settings, read_openai_settings,
    run_scan, run_scan_with_progress, set_openai_settings_at, settings_db_path_from_app,
    try_load_indexed_project_at, write_index, AskResponse, IndexProgressEvent,
    InspectSymbolResponse, ProjectOverview, PublicOpenAiSettings, ScanResult, UpdateOpenAiSettings,
};
use rusqlite::Connection;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

const SAMPLE_PROJECT_MARKER: &str = ".cppinsight-sample-v1";
const SAMPLE_PROJECT_FILES: &[(&str, &str)] = &[
    (
        "CMakeLists.txt",
        "cmake_minimum_required(VERSION 3.16)\nproject(cppinsight_sample LANGUAGES CXX)\nset(CMAKE_CXX_STANDARD 17)\nadd_executable(cppinsight_sample src/main.cpp src/engine.cpp src/worker.cpp)\ntarget_include_directories(cppinsight_sample PRIVATE include)\n",
    ),
    (
        "include/sample/engine.hpp",
        "#pragma once\n#include <string>\n\nnamespace sample {\nclass Engine {\npublic:\n    explicit Engine(std::string name);\n    std::string run(int jobs) const;\nprivate:\n    std::string name_;\n};\n}\n",
    ),
    (
        "src/engine.cpp",
        "#include \"sample/engine.hpp\"\n#include \"sample/worker.hpp\"\n\nnamespace sample {\nEngine::Engine(std::string name) : name_(std::move(name)) {}\nstd::string Engine::run(int jobs) const {\n    Worker worker(name_);\n    return worker.process(jobs);\n}\n}\n",
    ),
    (
        "include/sample/worker.hpp",
        "#pragma once\n#include <string>\n\nnamespace sample {\nclass Worker {\npublic:\n    explicit Worker(std::string owner);\n    std::string process(int jobs) const;\nprivate:\n    std::string owner_;\n};\n}\n",
    ),
    (
        "src/worker.cpp",
        "#include \"sample/worker.hpp\"\n#include <sstream>\n\nnamespace sample {\nWorker::Worker(std::string owner) : owner_(std::move(owner)) {}\nstd::string Worker::process(int jobs) const {\n    std::ostringstream output;\n    output << owner_ << \" processed \" << jobs << \" jobs\";\n    return output.str();\n}\n}\n",
    ),
    (
        "src/main.cpp",
        "#include \"sample/engine.hpp\"\n#include <iostream>\n\nint main() {\n    sample::Engine engine(\"demo\");\n    std::cout << engine.run(3) << '\\n';\n    return 0;\n}\n",
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
    fs::write(marker, "CppInsight AI bundled sample project v1\n")
        .map_err(|error| format!("write sample project marker failed: {error}"))?;
    Ok(())
}

#[tauri::command]
pub fn prepare_sample_project(app: AppHandle) -> Result<String, String> {
    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("failed to resolve sample project cache dir: {error}"))?;
    let root = cache_dir.join("sample-project-v1");
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
pub async fn index_project(app: AppHandle, path: String) -> Result<ProjectOverview, String> {
    let app_for_task = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let scan = run_scan_with_progress(&path, |event| {
            emit_index_progress(&app_for_task, &event);
        })?;

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
    .map_err(|error| format!("index task join error: {error}"))?
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
    load_project_overview(&connection, project_id)
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
    let settings_path = settings_db_path_from_app(&app)?;
    ask_project_at_with_settings(project_id, &question, &db_path, Some(&settings_path)).await
}

#[tauri::command]
pub async fn export_client_summary(
    app: tauri::AppHandle,
    root: String,
    project_id: i64,
) -> Result<String, String> {
    let db_path = index_db_path_for_root(&app, &PathBuf::from(&root))?;
    tauri::async_runtime::spawn_blocking(move || {
        let connection = open_database_at(&db_path)?;
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
    tauri::async_runtime::spawn_blocking(move || {
        let connection = open_database_at(&db_path)?;
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
    use super::materialize_sample_project;

    #[test]
    fn bundled_sample_project_is_complete_and_idempotent() {
        let directory = tempfile::tempdir().expect("temp directory");
        materialize_sample_project(directory.path()).expect("materialize sample");
        assert!(directory.path().join("CMakeLists.txt").is_file());
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
}
