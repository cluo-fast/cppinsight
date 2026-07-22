use crate::{
    ask_project_at, build_client_summary, default_db_path_from_app, emit_index_progress,
    index_scan_json_at, load_project_overview, open_database_at, open_database_from_app,
    overview_from_scan, run_scan, run_scan_with_progress, sql_error, write_index, AskResponse,
    IndexProgressEvent, ProjectOverview, ScanResult,
};
use rusqlite::params;
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

        let db_path = default_db_path_from_app(&app_for_task)?;
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
    let db_path = default_db_path_from_app(&app)?;
    index_scan_json_at(&scan_json, &db_path)
}

#[tauri::command]
pub fn get_project_overview(app: tauri::AppHandle, project_id: i64) -> Result<ProjectOverview, String> {
    let connection = open_database_from_app(&app)?;
    load_project_overview(&connection, project_id)
}

#[tauri::command]
pub fn set_openai_api_key(app: tauri::AppHandle, api_key: String) -> Result<(), String> {
    let connection = open_database_from_app(&app)?;
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
pub async fn ask_project(
    app: tauri::AppHandle,
    project_id: i64,
    question: String,
) -> Result<AskResponse, String> {
    let db_path = default_db_path_from_app(&app)?;
    ask_project_at(project_id, &question, &db_path).await
}

#[tauri::command]
pub fn export_client_summary(app: tauri::AppHandle, project_id: i64) -> Result<String, String> {
    let connection = open_database_from_app(&app)?;
    build_client_summary(&connection, project_id)
}
