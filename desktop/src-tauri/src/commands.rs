use crate::{
    ask_project_at_with_settings, build_client_summary, emit_index_progress, index_db_path_for_root,
    index_scan_json_at, load_project_overview, migrate_legacy_project_if_needed, open_database_at,
    overview_from_scan, run_scan, run_scan_with_progress, set_api_key_at, settings_db_path_from_app,
    write_index, AskResponse, IndexProgressEvent, ProjectOverview, ScanResult,
};
use std::path::PathBuf;
use tauri::AppHandle;

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
pub fn index_scan_json(app: tauri::AppHandle, scan_json: String) -> Result<ProjectOverview, String> {
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

#[tauri::command]
pub fn set_openai_api_key(app: tauri::AppHandle, api_key: String) -> Result<(), String> {
    let settings_path = settings_db_path_from_app(&app)?;
    set_api_key_at(&settings_path, &api_key)
}

#[tauri::command]
pub async fn ask_project(
    app: tauri::AppHandle,
    root: String,
    project_id: i64,
    question: String,
) -> Result<AskResponse, String> {
    let db_path = index_db_path_for_root(&app, &PathBuf::from(&root))?;
    let settings_path = settings_db_path_from_app(&app)?;
    ask_project_at_with_settings(project_id, &question, &db_path, Some(&settings_path)).await
}

#[tauri::command]
pub fn export_client_summary(
    app: tauri::AppHandle,
    root: String,
    project_id: i64,
) -> Result<String, String> {
    let db_path = index_db_path_for_root(&app, &PathBuf::from(&root))?;
    let connection = open_database_at(&db_path)?;
    build_client_summary(&connection, project_id)
}
