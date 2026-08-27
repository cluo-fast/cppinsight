use cppinsight_ai_lib::{
    ask_project_at, build_client_summary, default_db_path, find_project_id_by_root,
    index_scan_json_at, inspect_symbol_at, list_projects, open_database_at, project_index_db_path,
    set_api_key_at,
};
use std::env;
use std::fs;
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

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(1)
        }
    }
}
