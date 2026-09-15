use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SourcePreview {
    pub file: String,
    pub start_line: i64,
    pub end_line: i64,
    pub focus_line: i64,
    pub text: String,
    pub indexed_at: String,
    pub truncated: bool,
}

// Read only an existing indexed chunk. Never interpret the requested file as an OS path.
pub fn preview_source(
    connection: &Connection,
    project_id: i64,
    file: &str,
    line: i64,
) -> Result<SourcePreview, String> {
    if project_id <= 0 || line <= 0 || file.is_empty() || file.len() > 4096 {
        return Err("invalid source preview location".into());
    }
    let chunk = connection
        .query_row(
            "SELECT c.start_line, substr(c.text, 1, 32768), length(c.text) > 32768, p.indexed_at
         FROM chunks c JOIN projects p ON p.id = c.project_id
         WHERE c.project_id = ?1 AND c.file = ?2 AND c.start_line <= ?3 AND c.end_line >= ?3
         ORDER BY c.start_line DESC, c.id DESC LIMIT 1",
            params![project_id, file, line],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, bool>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "该位置没有索引源码片段，请重新索引或在编辑器中检查。".to_string())?;
    let (start, text, clipped, indexed_at) = chunk;
    let lines: Vec<_> = text.lines().collect();
    let offset = usize::try_from(line.saturating_sub(start)).unwrap_or(usize::MAX);
    if offset >= lines.len() {
        return Err("该位置超出可预览片段上限，请在编辑器中检查。".into());
    }
    let first = offset.saturating_sub(5);
    let end = (offset + 6).min(lines.len());
    let mut truncated = clipped;
    let excerpt = lines[first..end]
        .iter()
        .map(|line| {
            if line.chars().count() > 1000 {
                truncated = true;
            }
            line.chars().take(1000).collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(SourcePreview {
        file: file.to_string(),
        start_line: start + first as i64,
        end_line: start + end as i64 - 1,
        focus_line: line,
        text: excerpt,
        indexed_at,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn database() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE projects(id INTEGER, indexed_at TEXT);
            CREATE TABLE chunks(id INTEGER, project_id INTEGER, file TEXT, start_line INTEGER, end_line INTEGER, text TEXT);
            INSERT INTO projects VALUES(1, 'snapshot-time');").unwrap();
        db
    }
    #[test]
    fn preview_is_scoped_and_has_exact_line_numbers() {
        let db = database();
        let code = (1..=80)
            .map(|line| format!("line {line}\n"))
            .collect::<String>();
        db.execute("INSERT INTO chunks VALUES(1,1,'a.cpp',1,80,?1)", [code])
            .unwrap();
        let preview = preview_source(&db, 1, "a.cpp", 20).unwrap();
        assert_eq!((preview.start_line, preview.end_line), (15, 25));
        assert_eq!(preview.text.lines().nth(5), Some("line 20"));
        assert_eq!(preview.indexed_at, "snapshot-time");
        assert!(preview_source(&db, 2, "a.cpp", 20).is_err());
        assert!(preview_source(&db, 1, "../a.cpp", 20).is_err());
        assert!(preview_source(&db, 1, "a.cpp", 0).is_err());
        assert!(preview_source(&db, 1, "a.cpp", 81).is_err());
        assert_eq!(preview_source(&db, 1, "a.cpp", 80).unwrap().end_line, 80);
    }
    #[test]
    fn preview_bounds_long_unicode_lines_and_preserves_plain_text() {
        let db = database();
        db.execute(
            "INSERT INTO chunks VALUES(1,1,'a.cpp',1,2,?1)",
            [format!("<script>alert(1)</script>\n{}", "中".repeat(40000))],
        )
        .unwrap();
        let preview = preview_source(&db, 1, "a.cpp", 2).unwrap();
        assert!(preview.truncated);
        assert_eq!(preview.text.lines().nth(1).unwrap().chars().count(), 1000);
        assert!(preview.text.starts_with("<script>"));
    }
}
