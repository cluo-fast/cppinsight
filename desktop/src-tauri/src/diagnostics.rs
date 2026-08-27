use serde::Serialize;
use std::io::Write;
use std::path::Path;

const REDACTION_NOTICE: &str =
    "This report excludes source code, file paths, chat content, and API credentials.";

#[derive(Debug, Clone)]
pub struct DiagnosticProjectInput {
    pub root_path: String,
    pub index_status: String,
    pub file_count: u64,
    pub symbol_count: u64,
    pub include_count: u64,
    pub conversation_count: u64,
    pub mode: String,
    pub confidence: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticPlatform {
    pub os: String,
    pub arch: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticProject {
    pub project_id: String,
    pub index_status: String,
    pub file_count: u64,
    pub symbol_count: u64,
    pub include_count: u64,
    pub conversation_count: u64,
    pub mode: String,
    pub confidence: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticReport {
    pub schema_version: u32,
    pub generated_at: String,
    pub app_version: String,
    pub platform: DiagnosticPlatform,
    pub redaction_notice: String,
    pub projects: Vec<DiagnosticProject>,
}

pub fn build_report(
    generated_at: &str,
    app_version: &str,
    os: &str,
    arch: &str,
    projects: Vec<DiagnosticProjectInput>,
) -> DiagnosticReport {
    DiagnosticReport {
        schema_version: 1,
        generated_at: generated_at.to_string(),
        app_version: app_version.to_string(),
        platform: DiagnosticPlatform {
            os: os.to_string(),
            arch: arch.to_string(),
        },
        redaction_notice: REDACTION_NOTICE.to_string(),
        projects: projects
            .into_iter()
            .map(|project| DiagnosticProject {
                project_id: anonymous_project_id(&project.root_path),
                index_status: project.index_status,
                file_count: project.file_count,
                symbol_count: project.symbol_count,
                include_count: project.include_count,
                conversation_count: project.conversation_count,
                mode: project.mode,
                confidence: project.confidence,
            })
            .collect(),
    }
}

fn anonymous_project_id(root_path: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in root_path.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("project-{hash:016x}")
}

pub fn write_report(target: &Path, report: &DiagnosticReport) -> Result<(), String> {
    if !target.is_absolute()
        || target
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| !extension.eq_ignore_ascii_case("json"))
            .unwrap_or(true)
    {
        return Err("diagnostic export requires an absolute .json path".to_string());
    }

    let json = serde_json::to_vec_pretty(report)
        .map_err(|error| format!("failed to serialize diagnostic report: {error}"))?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(|error| format!("failed to create diagnostic report: {error}"))?;
    file.write_all(&json)
        .map_err(|error| format!("failed to write diagnostic report: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sensitive_project(root_path: &str) -> DiagnosticProjectInput {
        DiagnosticProjectInput {
            root_path: root_path.to_string(),
            index_status: "ready".to_string(),
            file_count: 12,
            symbol_count: 34,
            include_count: 56,
            conversation_count: 2,
            mode: "compile-db".to_string(),
            confidence: "high".to_string(),
        }
    }

    #[test]
    fn report_redacts_paths_and_keeps_only_aggregate_project_data() {
        let report = build_report(
            "2026-08-28T12:00:00Z",
            "0.1.0",
            "windows",
            "x86_64",
            vec![sensitive_project("C:\\Clients\\SecretCo\\main.cpp")],
        );
        let json = serde_json::to_string_pretty(&report).unwrap();

        assert!(!json.contains("SecretCo"));
        assert!(!json.contains("main.cpp"));
        assert_eq!(report.projects[0].file_count, 12);
        assert!(report.projects[0].project_id.starts_with("project-"));
        assert_eq!(report.schema_version, 1);
        assert_eq!(report.redaction_notice, REDACTION_NOTICE);
    }

    #[test]
    fn anonymous_project_id_is_stable_without_exposing_the_path() {
        let first = build_report(
            "now",
            "0.1.0",
            "windows",
            "x86_64",
            vec![sensitive_project("C:\\work\\alpha")],
        );
        let second = build_report(
            "later",
            "0.1.0",
            "windows",
            "x86_64",
            vec![sensitive_project("C:\\work\\alpha")],
        );
        let different = build_report(
            "later",
            "0.1.0",
            "windows",
            "x86_64",
            vec![sensitive_project("C:\\work\\beta")],
        );

        assert_eq!(first.projects[0].project_id, second.projects[0].project_id);
        assert_ne!(
            first.projects[0].project_id,
            different.projects[0].project_id
        );
    }

    #[test]
    fn export_requires_absolute_json_path_and_never_overwrites() {
        let report = build_report("now", "0.1.0", "windows", "x86_64", vec![]);
        assert!(write_report(std::path::Path::new("diagnostics.json"), &report).is_err());

        let directory = std::env::temp_dir().join(format!(
            "cppinsight-diagnostic-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("diagnostics.json");

        write_report(&path, &report).unwrap();
        assert!(write_report(&path, &report).is_err());
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(&path).unwrap())
                .unwrap()["schema_version"],
            1
        );

        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
