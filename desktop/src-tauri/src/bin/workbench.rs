use cppinsight_ai_lib::graph::{query_graph, GraphQueryRequest};
use cppinsight_ai_lib::{
    ask_project_at, build_client_summary, default_db_path, find_project_id_by_root,
    index_scan_json_at, inspect_symbol_at, list_projects, open_database_at, project_index_db_path,
    set_api_key_at,
};
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn print_usage() {
    eprintln!(
        "CppInsight Workbench CLI\n\n\
         Usage:\n\
           cppinsight-workbench index <scan.json> [--db <sqlite>]\n\
           cppinsight-workbench list [--db <sqlite>]\n\
           cppinsight-workbench inspect --root <project-root> <query> [--depth N] [--db <sqlite>]\n\
           cppinsight-workbench inspect --project <id> <query> [--depth N] [--db <sqlite>]\n\
           cppinsight-workbench graph --root <project-root> --request <request.json|-> [--db <sqlite>]\n\
           cppinsight-workbench ask --project <id> <question> [--db <sqlite>]\n\
           cppinsight-workbench ask --root <project-root> <question> [--db <sqlite>]\n\
           cppinsight-workbench summary --project <id> [--output <file>] [--db <sqlite>]\n\
           cppinsight-workbench summary --root <project-root> [--output <file>] [--db <sqlite>]\n\
           cppinsight-workbench set-key <api-key> [--db <sqlite>]\n\n\
         When --root is set and --db is omitted, the DB defaults to\n\
         <root>/.cppinsight/index.sqlite\n"
    );
}

fn take_flag(args: &mut Vec<String>, name: &str) -> Option<String> {
    if let Some(index) = args.iter().position(|item| item == name) {
        if index + 1 >= args.len() {
            return None;
        }
        let value = args[index + 1].clone();
        args.drain(index..=index + 1);
        return Some(value);
    }
    None
}

fn resolve_db(explicit_db: Option<PathBuf>, root: Option<&str>) -> PathBuf {
    if let Some(path) = explicit_db {
        return path;
    }
    if let Some(root) = root {
        return project_index_db_path(Path::new(root));
    }
    default_db_path()
}

fn resolve_project_id(
    args: &mut Vec<String>,
    db_path: &Path,
) -> Result<(i64, Option<String>), String> {
    let project_flag = take_flag(args, "--project");
    let root_flag = take_flag(args, "--root");
    if let Some(value) = project_flag {
        let id = value
            .parse::<i64>()
            .map_err(|_| "Invalid --project id".to_string())?;
        return Ok((id, root_flag));
    }
    if let Some(root) = root_flag {
        let id = find_project_id_by_root(db_path, &root)?;
        return Ok((id, Some(root)));
    }
    Err("requires --project <id> or --root <path>".to_string())
}

fn peek_flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|item| item == name)
        .and_then(|index| args.get(index + 1).map(String::as_str))
}

async fn run(mut args: Vec<String>) -> Result<(), String> {
    if args.is_empty() {
        print_usage();
        return Err("Missing command.".to_string());
    }

    let explicit_db = take_flag(&mut args, "--db").map(PathBuf::from);
    let command = args.remove(0);

    match command.as_str() {
        "index" => {
            let db_path = resolve_db(explicit_db, None);
            if args.is_empty() {
                return Err("Missing scan JSON path.".to_string());
            }
            let overview = index_scan_json_at(&args[0], &db_path)?;
            println!("Indexed project_id={}", overview.project_id);
            println!("root={}", overview.root);
            println!("files={}", overview.file_count);
            println!("symbols={}", overview.symbol_count);
            println!("mode={}", overview.mode);
            println!("db={}", db_path.display());
            Ok(())
        }
        "list" => {
            let db_path = resolve_db(explicit_db, None);
            let projects = list_projects(&db_path)?;
            if projects.is_empty() {
                println!("(no indexed projects)");
            }
            for (id, root, indexed_at) in projects {
                println!("{id}\t{indexed_at}\t{root}");
            }
            Ok(())
        }
        "inspect" => {
            let depth = match take_flag(&mut args, "--depth") {
                Some(value) => Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| "Invalid --depth".to_string())?,
                ),
                None => None,
            };
            let root_hint = peek_flag(&args, "--root").map(str::to_string);
            let db_path = resolve_db(explicit_db, root_hint.as_deref());
            let (project_id, _) =
                resolve_project_id(&mut args, &db_path).map_err(|e| format!("inspect {e}"))?;
            if args.is_empty() {
                return Err("Missing symbol query.".to_string());
            }
            let query = args.join(" ");
            let connection = open_database_at(&db_path)?;
            let response = inspect_symbol_at(&connection, project_id, &query, depth)?;
            let json = serde_json::to_string_pretty(&response)
                .map_err(|error| format!("failed to encode inspect JSON: {error}"))?;
            println!("{json}");
            Ok(())
        }
        "ask" => {
            let root_hint = peek_flag(&args, "--root").map(str::to_string);
            let db_path = resolve_db(explicit_db, root_hint.as_deref());
            let (project_id, _) =
                resolve_project_id(&mut args, &db_path).map_err(|e| format!("ask {e}"))?;
            if args.is_empty() {
                return Err("Missing question.".to_string());
            }
            let question = args.join(" ");
            let response = ask_project_at(project_id, &question, &db_path).await?;
            println!("{}", response.answer);
            if !response.citations.is_empty() {
                println!("\nCitations:");
                for citation in response.citations {
                    println!("- {}", citation.title);
                }
            }
            Ok(())
        }
        "summary" => {
            let root_hint = peek_flag(&args, "--root").map(str::to_string);
            let db_path = resolve_db(explicit_db, root_hint.as_deref());
            let output = take_flag(&mut args, "--output");
            let (project_id, _) =
                resolve_project_id(&mut args, &db_path).map_err(|e| format!("summary {e}"))?;
            let connection = open_database_at(&db_path)?;
            let summary = build_client_summary(&connection, project_id)?;
            if let Some(path) = output {
                fs::write(&path, &summary)
                    .map_err(|error| format!("failed to write {path}: {error}"))?;
                println!("Wrote {}", path);
            } else {
                print!("{summary}");
            }
            Ok(())
        }
        "set-key" => {
            let db_path = resolve_db(explicit_db, None);
            if args.is_empty() {
                return Err("Missing API key.".to_string());
            }
            set_api_key_at(&db_path, &args[0])?;
            println!("API key saved to operating-system credential store.");
            Ok(())
        }
        _ => {
            print_usage();
            Err(format!("Unknown command: {command}"))
        }
    }
}

async fn run_with_io<R: Read, W: Write>(
    args: Vec<String>,
    mut stdin: R,
    mut stdout: W,
) -> Result<(), String> {
    if args.first().map(String::as_str) != Some("graph") {
        return run(args).await;
    }

    let mut args = args;
    let explicit_db = take_flag(&mut args, "--db").map(PathBuf::from);
    let request_path = take_flag(&mut args, "--request")
        .ok_or_else(|| "graph requires --request <request.json|->".to_string())?;
    let root = take_flag(&mut args, "--root")
        .ok_or_else(|| "graph requires --root <project-root>".to_string())?;
    let command = args.remove(0);
    debug_assert_eq!(command, "graph");
    if !args.is_empty() {
        return Err(format!("unexpected graph arguments: {}", args.join(" ")));
    }

    let db_path = resolve_db(explicit_db, Some(&root));
    let project_id = find_project_id_by_root(&db_path, &root)?;
    let mut request_json = String::new();
    if request_path == "-" {
        stdin
            .read_to_string(&mut request_json)
            .map_err(|error| format!("failed to read graph request from stdin: {error}"))?;
    } else {
        request_json = fs::read_to_string(&request_path)
            .map_err(|error| format!("failed to read graph request {request_path}: {error}"))?;
    }
    let mut request: GraphQueryRequest = serde_json::from_str(&request_json)
        .map_err(|error| format!("failed to parse graph request JSON: {error}"))?;
    request.project_id = project_id;
    let connection = open_database_at(&db_path)?;
    let response = query_graph(&connection, request).map_err(|error| error.to_string())?;
    serde_json::to_writer_pretty(&mut stdout, &response)
        .map_err(|error| format!("failed to encode graph response JSON: {error}"))?;
    writeln!(stdout).map_err(|error| format!("failed to write graph response: {error}"))?;
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run_with_io(args, std::io::stdin(), std::io::stdout()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::run_with_io;
    use cppinsight_ai_lib::open_database_at;
    use serde_json::{json, Value};
    use std::io::Cursor;

    fn graph_database() -> (tempfile::TempDir, std::path::PathBuf, String, i64) {
        let temporary = tempfile::tempdir().unwrap();
        let database = temporary.path().join("graph-cli.sqlite");
        let connection = open_database_at(&database).unwrap();
        let root = "D:/cli-graph".to_string();
        connection
            .execute(
                "INSERT INTO projects(
                    root, indexed_at, file_count, symbol_count, include_count,
                    diagnostics_json, mode, confidence, index_schema_version,
                    generator_version, index_generation, index_complete
                 ) VALUES(?1, 'now', 0, 0, 0, '[]', 'heuristic', 'heuristic', 3, 'test', 1, 1)",
                [&root],
            )
            .unwrap();
        let project_id = connection.last_insert_rowid();
        connection
            .execute(
                "INSERT INTO module_relations(
                    project_id, source_module, target_module, relation_kind,
                    weight, confidence, evidence_count
                 ) VALUES(?1, 'module:app', 'module:core', 'include', 2, 'resolved', 2)",
                [project_id],
            )
            .unwrap();
        (temporary, database, root, project_id)
    }

    fn request(protocol_version: u32) -> String {
        json!({
            "protocol_version": protocol_version,
            "project_id": 999,
            "graph_kind": "module_dependencies",
            "root_node_ids": [],
            "direction": "both",
            "depth": 1,
            "filters": {},
            "max_nodes": 500,
            "path_target": null,
            "continuation": null
        })
        .to_string()
    }

    #[tokio::test]
    async fn graph_reads_stdin_overwrites_project_id_and_writes_only_json() {
        let (_temporary, database, root, project_id) = graph_database();
        let args = vec![
            "graph".to_string(),
            "--root".to_string(),
            root,
            "--request".to_string(),
            "-".to_string(),
            "--db".to_string(),
            database.display().to_string(),
        ];
        let mut output = Vec::new();
        run_with_io(args, Cursor::new(request(1)), &mut output)
            .await
            .unwrap();
        let response: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(response["project_id"], project_id);
        assert_eq!(response["edges"][0]["source"], "module:app");
    }

    #[tokio::test]
    async fn graph_propagates_protocol_validation_errors() {
        let (_temporary, database, root, _) = graph_database();
        let args = vec![
            "graph".to_string(),
            "--root".to_string(),
            root,
            "--request".to_string(),
            "-".to_string(),
            "--db".to_string(),
            database.display().to_string(),
        ];
        let error = run_with_io(args, Cursor::new(request(2)), Vec::new())
            .await
            .unwrap_err();
        assert!(error.contains("unsupported graph protocol version 2"));
    }
}
