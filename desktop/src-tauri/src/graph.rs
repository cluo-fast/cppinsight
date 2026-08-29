use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::error::Error;
use std::fmt::{Display, Formatter};

pub const GRAPH_PROTOCOL_VERSION: u32 = 1;
pub const DEFAULT_GRAPH_DEPTH: u32 = 2;
pub const MAX_GRAPH_DEPTH: u32 = 8;
pub const DEFAULT_MAX_NODES: usize = 500;
pub const MAX_GRAPH_NODES: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphKind {
    ModuleDependencies,
    FunctionCalls,
    ClassRelations,
    FileIncludes,
    EntryPaths,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphDirection {
    Incoming,
    Outgoing,
    #[default]
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphConfidence {
    Resolved,
    Precise,
    Hybrid,
    Heuristic,
    Ambiguous,
    Unresolved,
}

impl Display for GraphConfidence {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Resolved => "resolved",
            Self::Precise => "precise",
            Self::Hybrid => "hybrid",
            Self::Heuristic => "heuristic",
            Self::Ambiguous => "ambiguous",
            Self::Unresolved => "unresolved",
        };
        formatter.write_str(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphNodeKind {
    Module,
    File,
    Function,
    Class,
    Struct,
    Enum,
    Symbol,
    Entry,
    Unresolved,
    Group,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphFilters {
    #[serde(default)]
    pub module_ids: Vec<String>,
    #[serde(default)]
    pub file_paths: Vec<String>,
    #[serde(default)]
    pub symbol_kinds: Vec<String>,
    #[serde(default)]
    pub relation_kinds: Vec<String>,
    #[serde(default)]
    pub confidence: Vec<GraphConfidence>,
    #[serde(default)]
    pub search: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphQueryRequest {
    pub protocol_version: u32,
    pub project_id: i64,
    pub graph_kind: GraphKind,
    #[serde(default)]
    pub root_node_ids: Vec<String>,
    #[serde(default)]
    pub direction: GraphDirection,
    #[serde(default = "default_depth")]
    pub depth: u32,
    #[serde(default)]
    pub filters: GraphFilters,
    #[serde(default = "default_max_nodes")]
    pub max_nodes: usize,
    #[serde(default)]
    pub path_target: Option<String>,
    #[serde(default)]
    pub continuation: Option<String>,
}

fn default_depth() -> u32 {
    DEFAULT_GRAPH_DEPTH
}

fn default_max_nodes() -> usize {
    DEFAULT_MAX_NODES
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedGraphQuery {
    pub request: GraphQueryRequest,
    pub continuation_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphQueryError {
    Validation(String),
    Store(String),
}

impl Display for GraphQueryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Validation(message) => {
                write!(formatter, "graph query validation error: {message}")
            }
            Self::Store(message) => write!(formatter, "graph query store error: {message}"),
        }
    }
}

impl Error for GraphQueryError {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphPosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedGraphLayout {
    pub protocol_version: u32,
    pub project_id: i64,
    pub graph_kind: GraphKind,
    pub view_key: String,
    pub positions: BTreeMap<String, GraphPosition>,
    pub updated_at: String,
}

impl SavedGraphLayout {
    pub fn validate(&self) -> Result<(), GraphQueryError> {
        if self.protocol_version != GRAPH_PROTOCOL_VERSION {
            return Err(GraphQueryError::Validation(format!(
                "unsupported graph protocol version {}; expected {}",
                self.protocol_version, GRAPH_PROTOCOL_VERSION
            )));
        }
        if self.project_id <= 0 {
            return Err(GraphQueryError::Validation(
                "project_id must be positive".to_string(),
            ));
        }
        if self.view_key.trim().is_empty() {
            return Err(GraphQueryError::Validation(
                "view_key must not be empty".to_string(),
            ));
        }
        if self.view_key.len() > 128 {
            return Err(GraphQueryError::Validation(
                "view_key must be at most 128 bytes".to_string(),
            ));
        }
        if self.positions.len() > MAX_GRAPH_NODES {
            return Err(GraphQueryError::Validation(format!(
                "saved layout must contain at most {MAX_GRAPH_NODES} positions"
            )));
        }
        for (node_id, position) in &self.positions {
            if !is_valid_graph_node_id(node_id) {
                return Err(GraphQueryError::Validation(format!(
                    "invalid graph node id: {node_id}"
                )));
            }
            if !position.x.is_finite() || !position.y.is_finite() {
                return Err(GraphQueryError::Validation(format!(
                    "position for {node_id} must be finite"
                )));
            }
        }
        Ok(())
    }
}

impl GraphKind {
    pub(crate) fn as_storage_key(self) -> &'static str {
        match self {
            Self::ModuleDependencies => "module_dependencies",
            Self::FunctionCalls => "function_calls",
            Self::ClassRelations => "class_relations",
            Self::FileIncludes => "file_includes",
            Self::EntryPaths => "entry_paths",
        }
    }
}

impl GraphQueryRequest {
    pub fn validate(self) -> Result<ValidatedGraphQuery, GraphQueryError> {
        if self.protocol_version != GRAPH_PROTOCOL_VERSION {
            return Err(GraphQueryError::Validation(format!(
                "unsupported graph protocol version {}; expected {}",
                self.protocol_version, GRAPH_PROTOCOL_VERSION
            )));
        }
        if self.project_id <= 0 {
            return Err(GraphQueryError::Validation(
                "project_id must be positive".to_string(),
            ));
        }
        if self.depth > MAX_GRAPH_DEPTH {
            return Err(GraphQueryError::Validation(format!(
                "depth must be at most {MAX_GRAPH_DEPTH}"
            )));
        }
        if !(1..=MAX_GRAPH_NODES).contains(&self.max_nodes) {
            return Err(GraphQueryError::Validation(format!(
                "max_nodes must be between 1 and {MAX_GRAPH_NODES}"
            )));
        }
        for node_id in &self.root_node_ids {
            if !is_valid_graph_node_id(node_id) {
                return Err(GraphQueryError::Validation(format!(
                    "invalid graph node id: {node_id}"
                )));
            }
        }
        if let Some(path_target) = &self.path_target {
            if !is_valid_graph_node_id(path_target) {
                return Err(GraphQueryError::Validation(format!(
                    "invalid graph node id: {path_target}"
                )));
            }
        }
        let continuation_offset = match self.continuation.as_deref() {
            None => 0,
            Some(token) => parse_continuation(token)?,
        };
        Ok(ValidatedGraphQuery {
            request: self,
            continuation_offset,
        })
    }
}

fn is_valid_graph_node_id(node_id: &str) -> bool {
    if node_id.contains(['\n', '\r']) {
        return false;
    }
    if let Some(hash) = node_id.strip_prefix("sym:") {
        return hash.len() == 16 && hash.bytes().all(|byte| byte.is_ascii_hexdigit());
    }
    ["module:", "file:", "entry:", "group:", "unresolved:"]
        .iter()
        .any(|prefix| {
            node_id
                .strip_prefix(prefix)
                .is_some_and(|suffix| !suffix.is_empty())
        })
}

fn parse_continuation(token: &str) -> Result<usize, GraphQueryError> {
    token
        .strip_prefix("v1:")
        .and_then(|offset| offset.parse::<usize>().ok())
        .ok_or_else(|| GraphQueryError::Validation(format!("invalid continuation token: {token}")))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub kind: GraphNodeKind,
    pub label: String,
    pub qualified_name: String,
    pub file: String,
    pub line: u64,
    pub module_id: String,
    pub confidence: GraphConfidence,
    pub ambiguous: bool,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub kind: String,
    pub weight: u64,
    pub confidence: GraphConfidence,
    pub ambiguous: bool,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphGroup {
    pub id: String,
    pub label: String,
    pub kind: String,
    #[serde(default)]
    pub node_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphEvidence {
    pub id: String,
    pub edge_id: String,
    pub file: String,
    pub line: u64,
    pub detail: String,
    pub confidence: GraphConfidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphConfidenceSummary {
    pub overall: GraphConfidence,
    pub resolved: usize,
    pub precise: usize,
    pub hybrid: usize,
    pub heuristic: usize,
    pub ambiguous: usize,
    pub unresolved: usize,
}

impl Default for GraphConfidenceSummary {
    fn default() -> Self {
        Self {
            overall: GraphConfidence::Heuristic,
            resolved: 0,
            precise: 0,
            hybrid: 0,
            heuristic: 0,
            ambiguous: 0,
            unresolved: 0,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphStatistics {
    pub original_node_count: usize,
    pub returned_node_count: usize,
    pub original_edge_count: usize,
    pub returned_edge_count: usize,
    pub cycle_count: usize,
    pub unresolved_edge_count: usize,
    pub ambiguous_edge_count: usize,
    #[serde(default)]
    pub central_node_ids: Vec<String>,
    #[serde(default)]
    pub shortest_path_node_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphQueryResponse {
    pub protocol_version: u32,
    pub project_id: i64,
    pub graph_kind: GraphKind,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub groups: Vec<GraphGroup>,
    pub evidence: Vec<GraphEvidence>,
    pub confidence: GraphConfidenceSummary,
    pub statistics: GraphStatistics,
    pub truncated: bool,
    pub truncation_reason: Option<String>,
    pub continuation: Option<String>,
}

impl GraphQueryResponse {
    pub fn empty(project_id: i64, graph_kind: GraphKind) -> Self {
        Self {
            protocol_version: GRAPH_PROTOCOL_VERSION,
            project_id,
            graph_kind,
            nodes: Vec::new(),
            edges: Vec::new(),
            groups: Vec::new(),
            evidence: Vec::new(),
            confidence: GraphConfidenceSummary::default(),
            statistics: GraphStatistics::default(),
            truncated: false,
            truncation_reason: None,
            continuation: None,
        }
    }
}

#[derive(Default)]
struct CanonicalGraph {
    nodes: BTreeMap<String, GraphNode>,
    edges: Vec<GraphEdge>,
    evidence: BTreeMap<String, GraphEvidence>,
    default_roots: Vec<String>,
}

type ModuleEdgeKey = (String, String, String);
type ModuleEvidence = (String, u64, GraphConfidence, String);

fn store_error(error: rusqlite::Error) -> GraphQueryError {
    GraphQueryError::Store(error.to_string())
}

fn parse_confidence(value: &str) -> GraphConfidence {
    match value {
        "resolved" => GraphConfidence::Resolved,
        "precise" => GraphConfidence::Precise,
        "hybrid" => GraphConfidence::Hybrid,
        "ambiguous" => GraphConfidence::Ambiguous,
        "unresolved" => GraphConfidence::Unresolved,
        _ => GraphConfidence::Heuristic,
    }
}

fn symbol_node_kind(value: &str) -> GraphNodeKind {
    match value {
        "function" | "method" | "constructor" | "destructor" => GraphNodeKind::Function,
        "class" => GraphNodeKind::Class,
        "struct" => GraphNodeKind::Struct,
        "enum" => GraphNodeKind::Enum,
        _ => GraphNodeKind::Symbol,
    }
}

fn module_for_file(file: &str) -> String {
    let normalized = file.replace('\\', "/");
    let top = normalized
        .split('/')
        .find(|part| !part.is_empty() && *part != ".")
        .unwrap_or(".");
    format!("module:{top}")
}

fn stable_id(prefix: &str, parts: &[&str]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for part in parts {
        for byte in part.bytes().chain(std::iter::once(0xff)) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    format!("{prefix}:{hash:016x}")
}

fn load_symbol_nodes(
    connection: &Connection,
    project_id: i64,
    graph: &mut CanonicalGraph,
) -> Result<(), GraphQueryError> {
    let mut statement = connection
        .prepare(
            "SELECT stable_key, kind, name, qualified_name, file, line, confidence
             FROM symbols WHERE project_id = ?1 AND stable_key != ''
             ORDER BY stable_key, is_definition DESC, file, line",
        )
        .map_err(store_error)?;
    let rows = statement
        .query_map(params![project_id], |row| {
            Ok(GraphNode {
                id: row.get(0)?,
                kind: symbol_node_kind(row.get::<_, String>(1)?.as_str()),
                label: row.get(2)?,
                qualified_name: row.get(3)?,
                file: row.get(4)?,
                line: row.get::<_, i64>(5)? as u64,
                module_id: String::new(),
                confidence: parse_confidence(row.get::<_, String>(6)?.as_str()),
                ambiguous: false,
                metadata: BTreeMap::new(),
            })
        })
        .map_err(store_error)?;
    for row in rows {
        let mut node = row.map_err(store_error)?;
        node.module_id = module_for_file(&node.file);
        graph.nodes.entry(node.id.clone()).or_insert(node);
    }
    Ok(())
}

fn unresolved_node(id: String, label: String, file: String, line: u64) -> GraphNode {
    GraphNode {
        id,
        kind: GraphNodeKind::Unresolved,
        qualified_name: label.clone(),
        label,
        module_id: module_for_file(&file),
        file,
        line,
        confidence: GraphConfidence::Unresolved,
        ambiguous: false,
        metadata: BTreeMap::new(),
    }
}

fn load_symbol_relations(
    connection: &Connection,
    project_id: i64,
    relation_kinds: &[&str],
    graph: &mut CanonicalGraph,
) -> Result<(), GraphQueryError> {
    load_symbol_nodes(connection, project_id, graph)?;
    let mut statement = connection
        .prepare(
            "SELECT id, source_key, target_key, target_name, relation_kind,
                    evidence_file, evidence_line, confidence, ambiguous,
                    candidate_target_keys_json
             FROM symbol_relations WHERE project_id = ?1 ORDER BY id",
        )
        .map_err(store_error)?;
    let rows = statement
        .query_map(params![project_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)? as u64,
                row.get::<_, String>(7)?,
                row.get::<_, i64>(8)? != 0,
                row.get::<_, String>(9)?,
            ))
        })
        .map_err(store_error)?;
    for row in rows {
        let (
            row_id,
            source,
            stored_target,
            target_name,
            kind,
            file,
            line,
            confidence,
            ambiguous,
            candidates_json,
        ) = row.map_err(store_error)?;
        if !relation_kinds.contains(&kind.as_str()) || !graph.nodes.contains_key(&source) {
            continue;
        }
        let confidence = parse_confidence(&confidence);
        let target = if !stored_target.is_empty() {
            stored_target
        } else {
            stable_id(
                "unresolved",
                &[&kind, &target_name, &file, &line.to_string()],
            )
        };
        if !graph.nodes.contains_key(&target) {
            let mut node = unresolved_node(target.clone(), target_name.clone(), file.clone(), line);
            node.ambiguous = ambiguous;
            if !candidates_json.is_empty() && candidates_json != "[]" {
                node.metadata
                    .insert("candidates".to_string(), candidates_json);
            }
            graph.nodes.insert(target.clone(), node);
        }
        let edge_id = stable_id("edge", &[&source, &target, &kind, &row_id.to_string()]);
        let evidence_id = stable_id("ev", &[&edge_id, &file, &line.to_string()]);
        graph.evidence.insert(
            evidence_id.clone(),
            GraphEvidence {
                id: evidence_id.clone(),
                edge_id: edge_id.clone(),
                file,
                line,
                detail: format!("{kind} {target_name}"),
                confidence,
            },
        );
        graph.edges.push(GraphEdge {
            id: edge_id,
            source,
            target,
            kind,
            weight: 1,
            confidence,
            ambiguous,
            evidence_ids: vec![evidence_id],
        });
    }
    Ok(())
}

fn load_modules(
    connection: &Connection,
    project_id: i64,
    graph: &mut CanonicalGraph,
) -> Result<(), GraphQueryError> {
    let mut evidence_by_edge: HashMap<ModuleEdgeKey, Vec<ModuleEvidence>> = HashMap::new();
    let mut relation_statement = connection
        .prepare(
            "SELECT s_source.file, sr.evidence_line, sr.relation_kind, sr.confidence,
                    s_target.file, sr.target_name
             FROM symbol_relations sr
             JOIN symbols s_source ON s_source.project_id = sr.project_id AND s_source.stable_key = sr.source_key
             JOIN symbols s_target ON s_target.project_id = sr.project_id AND s_target.stable_key = sr.target_key
             WHERE sr.project_id = ?1 AND sr.source_key != '' AND sr.target_key != ''
             ORDER BY sr.id",
        )
        .map_err(store_error)?;
    let relation_rows = relation_statement
        .query_map(params![project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(store_error)?;
    for row in relation_rows {
        let (source_file, line, kind, confidence, target_file, target_name) =
            row.map_err(store_error)?;
        let key = (
            module_for_file(&source_file),
            module_for_file(&target_file),
            kind.clone(),
        );
        evidence_by_edge.entry(key).or_default().push((
            source_file,
            line,
            parse_confidence(&confidence),
            format!("{kind} {target_name}"),
        ));
    }
    let mut include_statement = connection
        .prepare(
            "SELECT file, line, target, resolved_path
             FROM includes WHERE project_id = ?1 AND resolved = 1 ORDER BY id",
        )
        .map_err(store_error)?;
    let include_rows = include_statement
        .query_map(params![project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(store_error)?;
    for row in include_rows {
        let (source_file, line, target_name, resolved_path) = row.map_err(store_error)?;
        let key = (
            module_for_file(&source_file),
            module_for_file(&resolved_path),
            "include".to_string(),
        );
        evidence_by_edge.entry(key).or_default().push((
            source_file,
            line,
            GraphConfidence::Resolved,
            format!("include {target_name}"),
        ));
    }
    let mut statement = connection
        .prepare(
            "SELECT source_module, target_module, relation_kind, weight, confidence
             FROM module_relations WHERE project_id = ?1
             ORDER BY source_module, target_module, relation_kind",
        )
        .map_err(store_error)?;
    let rows = statement
        .query_map(params![project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)? as u64,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(store_error)?;
    for row in rows {
        let (source, target, kind, weight, confidence) = row.map_err(store_error)?;
        for module_id in [&source, &target] {
            graph
                .nodes
                .entry(module_id.clone())
                .or_insert_with(|| GraphNode {
                    id: module_id.clone(),
                    kind: GraphNodeKind::Module,
                    label: module_id.trim_start_matches("module:").to_string(),
                    qualified_name: module_id.clone(),
                    file: String::new(),
                    line: 0,
                    module_id: module_id.clone(),
                    confidence: GraphConfidence::Resolved,
                    ambiguous: false,
                    metadata: BTreeMap::new(),
                });
        }
        let edge_id = stable_id("edge", &[&source, &target, &kind]);
        let evidence_rows = evidence_by_edge
            .remove(&(source.clone(), target.clone(), kind.clone()))
            .unwrap_or_default();
        let mut evidence_ids = Vec::new();
        for (index, (file, line, evidence_confidence, detail)) in
            evidence_rows.into_iter().enumerate()
        {
            let evidence_id = stable_id(
                "ev",
                &[&edge_id, &file, &line.to_string(), &index.to_string()],
            );
            graph.evidence.insert(
                evidence_id.clone(),
                GraphEvidence {
                    id: evidence_id.clone(),
                    edge_id: edge_id.clone(),
                    file,
                    line,
                    detail,
                    confidence: evidence_confidence,
                },
            );
            evidence_ids.push(evidence_id);
        }
        if evidence_ids.is_empty() {
            let evidence_id = stable_id("ev", &[&edge_id, "aggregate"]);
            graph.evidence.insert(
                evidence_id.clone(),
                GraphEvidence {
                    id: evidence_id.clone(),
                    edge_id: edge_id.clone(),
                    file: String::new(),
                    line: 0,
                    detail: format!("aggregated {kind} relation ({weight} observations)"),
                    confidence: parse_confidence(&confidence),
                },
            );
            evidence_ids.push(evidence_id);
        }
        graph.edges.push(GraphEdge {
            id: edge_id,
            source,
            target,
            kind,
            weight,
            confidence: parse_confidence(&confidence),
            ambiguous: confidence == "ambiguous",
            evidence_ids,
        });
    }
    Ok(())
}

fn load_includes(
    connection: &Connection,
    project_id: i64,
    graph: &mut CanonicalGraph,
) -> Result<(), GraphQueryError> {
    let mut statement = connection
        .prepare(
            "SELECT id, file, line, target, resolved, resolved_path
             FROM includes WHERE project_id = ?1 ORDER BY file, line, id",
        )
        .map_err(store_error)?;
    let rows = statement
        .query_map(params![project_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? as u64,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)? != 0,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(store_error)?;
    for row in rows {
        let (row_id, file, line, target_name, resolved, resolved_path) =
            row.map_err(store_error)?;
        let source = format!("file:{}", file.replace('\\', "/"));
        let target_path = if resolved && !resolved_path.is_empty() {
            resolved_path.replace('\\', "/")
        } else {
            target_name.clone()
        };
        let target = if resolved && !resolved_path.is_empty() {
            format!("file:{target_path}")
        } else {
            // Unresolved include targets must remain stable when source lines
            // move; do not bake the evidence line into the node identity.
            stable_id("unresolved", &["include", &target_name])
        };
        for (id, path, confidence) in [
            (&source, file.as_str(), GraphConfidence::Resolved),
            (
                &target,
                target_path.as_str(),
                if resolved {
                    GraphConfidence::Resolved
                } else {
                    GraphConfidence::Unresolved
                },
            ),
        ] {
            graph.nodes.entry(id.clone()).or_insert_with(|| GraphNode {
                id: id.clone(),
                kind: if resolved {
                    GraphNodeKind::File
                } else {
                    GraphNodeKind::Unresolved
                },
                label: path.rsplit('/').next().unwrap_or(path).to_string(),
                qualified_name: path.to_string(),
                file: path.to_string(),
                line: 0,
                module_id: module_for_file(path),
                confidence,
                ambiguous: false,
                metadata: BTreeMap::new(),
            });
        }
        let edge_id = stable_id("edge", &[&source, &target, "includes", &row_id.to_string()]);
        let evidence_id = stable_id("ev", &[&edge_id, &file, &line.to_string()]);
        let confidence = if resolved {
            GraphConfidence::Resolved
        } else {
            GraphConfidence::Unresolved
        };
        graph.evidence.insert(
            evidence_id.clone(),
            GraphEvidence {
                id: evidence_id.clone(),
                edge_id: edge_id.clone(),
                file,
                line,
                detail: format!("include {target_name}"),
                confidence,
            },
        );
        graph.edges.push(GraphEdge {
            id: edge_id,
            source,
            target,
            kind: "includes".to_string(),
            weight: 1,
            confidence,
            ambiguous: false,
            evidence_ids: vec![evidence_id],
        });
    }
    Ok(())
}

fn load_canonical_graph(
    connection: &Connection,
    project_id: i64,
    graph_kind: GraphKind,
) -> Result<CanonicalGraph, GraphQueryError> {
    let complete = connection
        .query_row(
            "SELECT index_complete FROM projects WHERE id = ?1",
            params![project_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(store_error)?;
    if complete == 0 {
        return Err(GraphQueryError::Store(
            "project index generation is incomplete".to_string(),
        ));
    }

    let mut graph = CanonicalGraph::default();
    match graph_kind {
        GraphKind::ModuleDependencies => load_modules(connection, project_id, &mut graph)?,
        GraphKind::FunctionCalls => {
            load_symbol_relations(connection, project_id, &["calls"], &mut graph)?;
        }
        GraphKind::ClassRelations => {
            load_symbol_relations(
                connection,
                project_id,
                &["inherits", "implements", "contains", "uses"],
                &mut graph,
            )?;
        }
        GraphKind::FileIncludes => load_includes(connection, project_id, &mut graph)?,
        GraphKind::EntryPaths => {
            load_symbol_relations(connection, project_id, &["calls"], &mut graph)?;
            let mut statement = connection
                .prepare(
                    "SELECT symbol_key FROM entry_points
                     WHERE project_id = ?1 ORDER BY symbol_key",
                )
                .map_err(store_error)?;
            let roots = statement
                .query_map(params![project_id], |row| row.get::<_, String>(0))
                .map_err(store_error)?;
            for root in roots {
                let root = root.map_err(store_error)?;
                if graph.nodes.contains_key(&root) {
                    graph.default_roots.push(root);
                }
            }
        }
    }
    graph.edges.sort_by(|left, right| left.id.cmp(&right.id));
    graph.default_roots.sort();
    graph.default_roots.dedup();
    Ok(graph)
}

fn edge_allowed(edge: &GraphEdge, filters: &GraphFilters) -> bool {
    (filters.relation_kinds.is_empty() || filters.relation_kinds.contains(&edge.kind))
        && (filters.confidence.is_empty() || filters.confidence.contains(&edge.confidence))
}

fn node_allowed(node: &GraphNode, filters: &GraphFilters) -> bool {
    if !filters.module_ids.is_empty() && !filters.module_ids.contains(&node.module_id) {
        return false;
    }
    if !filters.file_paths.is_empty() && !filters.file_paths.contains(&node.file) {
        return false;
    }
    let kind = match node.kind {
        GraphNodeKind::Module => "module",
        GraphNodeKind::File => "file",
        GraphNodeKind::Function => "function",
        GraphNodeKind::Class => "class",
        GraphNodeKind::Struct => "struct",
        GraphNodeKind::Enum => "enum",
        GraphNodeKind::Entry => "entry",
        GraphNodeKind::Unresolved => "unresolved",
        GraphNodeKind::Group => "group",
        GraphNodeKind::Symbol => "symbol",
    };
    if !filters.symbol_kinds.is_empty()
        && !filters
            .symbol_kinds
            .iter()
            .any(|candidate| candidate == kind)
    {
        return false;
    }
    if let Some(search) = filters
        .search
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        let needle = search.to_lowercase();
        if !node.label.to_lowercase().contains(&needle)
            && !node.qualified_name.to_lowercase().contains(&needle)
            && !node.file.to_lowercase().contains(&needle)
        {
            return false;
        }
    }
    true
}

fn adjacency(
    edges: &[GraphEdge],
    direction: GraphDirection,
) -> HashMap<&str, Vec<(&str, &GraphEdge)>> {
    let mut result: HashMap<&str, Vec<(&str, &GraphEdge)>> = HashMap::new();
    for edge in edges {
        if direction != GraphDirection::Incoming {
            result
                .entry(edge.source.as_str())
                .or_default()
                .push((edge.target.as_str(), edge));
        }
        if direction != GraphDirection::Outgoing {
            result
                .entry(edge.target.as_str())
                .or_default()
                .push((edge.source.as_str(), edge));
        }
    }
    for neighbors in result.values_mut() {
        neighbors.sort_by(|left, right| left.0.cmp(right.0).then(left.1.id.cmp(&right.1.id)));
    }
    result
}

fn traverse(
    nodes: &BTreeMap<String, GraphNode>,
    edges: &[GraphEdge],
    roots: &[String],
    direction: GraphDirection,
    depth: u32,
) -> BTreeSet<String> {
    if roots.is_empty() {
        return nodes.keys().cloned().collect();
    }
    let adjacency = adjacency(edges, direction);
    let mut visited = BTreeSet::new();
    let mut queue = VecDeque::new();
    for root in roots {
        if nodes.contains_key(root) && visited.insert(root.clone()) {
            queue.push_back((root.clone(), 0_u32));
        }
    }
    while let Some((node, distance)) = queue.pop_front() {
        if distance >= depth {
            continue;
        }
        if let Some(neighbors) = adjacency.get(node.as_str()) {
            for (neighbor, _) in neighbors {
                if visited.insert((*neighbor).to_string()) {
                    queue.push_back(((*neighbor).to_string(), distance + 1));
                }
            }
        }
    }
    visited
}

fn shortest_path(
    edges: &[GraphEdge],
    roots: &[String],
    target: Option<&str>,
    direction: GraphDirection,
) -> Vec<String> {
    let Some(target) = target else {
        return Vec::new();
    };
    let adjacency = adjacency(edges, direction);
    let mut queue = VecDeque::new();
    let mut predecessor: HashMap<String, Option<String>> = HashMap::new();
    let mut sorted_roots = roots.to_vec();
    sorted_roots.sort();
    for root in sorted_roots {
        if predecessor.insert(root.clone(), None).is_none() {
            queue.push_back(root);
        }
    }
    while let Some(node) = queue.pop_front() {
        if node == target {
            let mut path = Vec::new();
            let mut cursor = Some(node);
            while let Some(current) = cursor {
                cursor = predecessor.get(&current).cloned().flatten();
                path.push(current);
            }
            path.reverse();
            return path;
        }
        if let Some(neighbors) = adjacency.get(node.as_str()) {
            for (neighbor, _) in neighbors {
                if !predecessor.contains_key(*neighbor) {
                    predecessor.insert((*neighbor).to_string(), Some(node.clone()));
                    queue.push_back((*neighbor).to_string());
                }
            }
        }
    }
    Vec::new()
}

fn cycle_components(nodes: &BTreeSet<String>, edges: &[GraphEdge]) -> Vec<Vec<String>> {
    let mut outgoing = HashMap::<&str, Vec<&str>>::new();
    let mut incoming = HashMap::<&str, Vec<&str>>::new();
    let mut self_loops = HashSet::new();
    for edge in edges {
        outgoing
            .entry(edge.source.as_str())
            .or_default()
            .push(edge.target.as_str());
        incoming
            .entry(edge.target.as_str())
            .or_default()
            .push(edge.source.as_str());
        if edge.source == edge.target {
            self_loops.insert(edge.source.as_str());
        }
    }
    for neighbors in outgoing.values_mut() {
        neighbors.sort();
        neighbors.dedup();
    }
    for neighbors in incoming.values_mut() {
        neighbors.sort();
        neighbors.dedup();
    }

    let mut visited = HashSet::new();
    let mut finish_order = Vec::new();
    for start in nodes {
        if visited.contains(start.as_str()) {
            continue;
        }
        let mut stack = vec![(start.as_str(), false)];
        while let Some((current, expanded)) = stack.pop() {
            if expanded {
                finish_order.push(current);
                continue;
            }
            if !visited.insert(current) {
                continue;
            }
            stack.push((current, true));
            if let Some(neighbors) = outgoing.get(current) {
                for neighbor in neighbors.iter().rev() {
                    if nodes.contains(*neighbor) && !visited.contains(*neighbor) {
                        stack.push((neighbor, false));
                    }
                }
            }
        }
    }

    visited.clear();
    let mut components = Vec::new();
    while let Some(start) = finish_order.pop() {
        if !visited.insert(start) {
            continue;
        }
        let mut component = Vec::new();
        let mut stack = vec![start];
        while let Some(current) = stack.pop() {
            component.push(current.to_string());
            if let Some(neighbors) = incoming.get(current) {
                for neighbor in neighbors {
                    if nodes.contains(*neighbor) && visited.insert(neighbor) {
                        stack.push(neighbor);
                    }
                }
            }
        }
        if component.len() > 1 || self_loops.contains(start) {
            component.sort();
            components.push(component);
        }
    }
    components.sort();
    components
}

fn top_degree_roots(nodes: &BTreeSet<String>, edges: &[GraphEdge]) -> Vec<String> {
    let mut degrees = nodes
        .iter()
        .map(|id| (id.clone(), 0_usize))
        .collect::<HashMap<_, _>>();
    for edge in edges {
        *degrees.entry(edge.source.clone()).or_default() += 1;
        *degrees.entry(edge.target.clone()).or_default() += 1;
    }
    let mut ranked = degrees.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
    ranked.into_iter().take(10).map(|(id, _)| id).collect()
}

fn confidence_summary(edges: &[GraphEdge]) -> GraphConfidenceSummary {
    let mut summary = GraphConfidenceSummary::default();
    for edge in edges {
        match edge.confidence {
            GraphConfidence::Resolved => summary.resolved += 1,
            GraphConfidence::Precise => summary.precise += 1,
            GraphConfidence::Hybrid => summary.hybrid += 1,
            GraphConfidence::Heuristic => summary.heuristic += 1,
            GraphConfidence::Ambiguous => summary.ambiguous += 1,
            GraphConfidence::Unresolved => summary.unresolved += 1,
        }
    }
    summary.overall = if summary.unresolved > 0 {
        GraphConfidence::Unresolved
    } else if summary.ambiguous > 0 {
        GraphConfidence::Ambiguous
    } else if summary.heuristic > 0 {
        GraphConfidence::Heuristic
    } else if summary.hybrid > 0 {
        GraphConfidence::Hybrid
    } else if summary.precise > 0 {
        GraphConfidence::Precise
    } else {
        GraphConfidence::Resolved
    };
    summary
}

pub fn query_graph(
    connection: &Connection,
    request: GraphQueryRequest,
) -> Result<GraphQueryResponse, GraphQueryError> {
    let validated = request.validate()?;
    let request = validated.request;
    let graph = load_canonical_graph(connection, request.project_id, request.graph_kind)?;
    if let Some(target) = request.path_target.as_deref() {
        if !graph.nodes.contains_key(target) {
            return Err(GraphQueryError::Validation(format!(
                "path_target does not exist in this graph: {target}"
            )));
        }
    }
    let candidate_nodes = graph
        .nodes
        .iter()
        .filter(|(_, node)| {
            node_allowed(node, &request.filters)
                && (node.kind != GraphNodeKind::Unresolved
                    || request.filters.confidence.is_empty()
                    || request
                        .filters
                        .confidence
                        .contains(&GraphConfidence::Unresolved))
        })
        .map(|(id, _)| id.clone())
        .collect::<BTreeSet<_>>();
    let allowed_edges = graph
        .edges
        .iter()
        .filter(|edge| edge_allowed(edge, &request.filters))
        .filter(|edge| {
            candidate_nodes.contains(&edge.source) && candidate_nodes.contains(&edge.target)
        })
        .cloned()
        .collect::<Vec<_>>();
    let roots = if !request.root_node_ids.is_empty() {
        request.root_node_ids.clone()
    } else if !graph.default_roots.is_empty() {
        graph.default_roots.clone()
    } else {
        top_degree_roots(&candidate_nodes, &allowed_edges)
    };
    let traversal_depth = if request.root_node_ids.is_empty() && graph.default_roots.is_empty() {
        1
    } else {
        request.depth
    };
    let traversed = traverse(
        &graph.nodes,
        &allowed_edges,
        &roots,
        request.direction,
        traversal_depth,
    );
    let visible = traversed
        .into_iter()
        .filter(|id| candidate_nodes.contains(id))
        .collect::<BTreeSet<_>>();
    let visible_edges = allowed_edges
        .into_iter()
        .filter(|edge| visible.contains(&edge.source) && visible.contains(&edge.target))
        .collect::<Vec<_>>();
    let path = shortest_path(
        &visible_edges,
        &roots,
        request.path_target.as_deref(),
        request.direction,
    );
    let cycles = cycle_components(&visible, &visible_edges);

    let all_node_ids = visible.into_iter().collect::<Vec<_>>();
    let start = validated.continuation_offset.min(all_node_ids.len());
    let end = (start + request.max_nodes).min(all_node_ids.len());
    let page_ids = all_node_ids[start..end]
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let nodes = page_ids
        .iter()
        .filter_map(|id| graph.nodes.get(id).cloned())
        .collect::<Vec<_>>();
    let edges = visible_edges
        .iter()
        .filter(|edge| page_ids.contains(&edge.source) && page_ids.contains(&edge.target))
        .cloned()
        .collect::<Vec<_>>();
    let evidence_ids = edges
        .iter()
        .flat_map(|edge| edge.evidence_ids.iter().cloned())
        .collect::<BTreeSet<_>>();
    let evidence = evidence_ids
        .into_iter()
        .filter_map(|id| graph.evidence.get(&id).cloned())
        .collect::<Vec<_>>();
    let mut degree = HashMap::<String, usize>::new();
    for edge in &visible_edges {
        *degree.entry(edge.source.clone()).or_default() += 1;
        *degree.entry(edge.target.clone()).or_default() += 1;
    }
    let mut central = degree.into_iter().collect::<Vec<_>>();
    central.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
    let groups = cycles
        .iter()
        .filter(|component| component.iter().all(|id| page_ids.contains(id)))
        .map(|component| GraphGroup {
            id: stable_id(
                "group",
                &component.iter().map(String::as_str).collect::<Vec<_>>(),
            ),
            label: format!("Cycle ({})", component.len()),
            kind: "cycle".to_string(),
            node_ids: component.clone(),
        })
        .collect();
    let truncated = end < all_node_ids.len();
    Ok(GraphQueryResponse {
        protocol_version: GRAPH_PROTOCOL_VERSION,
        project_id: request.project_id,
        graph_kind: request.graph_kind,
        nodes,
        edges: edges.clone(),
        groups,
        evidence,
        confidence: confidence_summary(&visible_edges),
        statistics: GraphStatistics {
            original_node_count: all_node_ids.len(),
            returned_node_count: page_ids.len(),
            original_edge_count: visible_edges.len(),
            returned_edge_count: edges.len(),
            cycle_count: cycles.len(),
            unresolved_edge_count: visible_edges
                .iter()
                .filter(|edge| edge.confidence == GraphConfidence::Unresolved)
                .count(),
            ambiguous_edge_count: visible_edges.iter().filter(|edge| edge.ambiguous).count(),
            central_node_ids: central.into_iter().take(10).map(|(id, _)| id).collect(),
            shortest_path_node_ids: path,
        },
        truncated,
        truncation_reason: truncated.then(|| "node_limit".to_string()),
        continuation: truncated.then(|| format!("v1:{end}")),
    })
}
