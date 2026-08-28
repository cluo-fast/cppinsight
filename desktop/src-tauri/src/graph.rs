use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
    Hybrid,
    Heuristic,
    Ambiguous,
    Unresolved,
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
