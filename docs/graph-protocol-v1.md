# CppInsight Graph Protocol v1

CppInsight desktop、Workbench CLI 和 MCP 服务共享同一份本地关系图协议。协议版本为 `1`，图数据来自项目根目录下的 `.cppinsight/index.sqlite`；查询不需要 AI 网络调用。

## Request

```json
{
  "protocol_version": 1,
  "project_id": 7,
  "graph_kind": "function_calls",
  "root_node_ids": ["sym:0123456789abcdef"],
  "direction": "outgoing",
  "depth": 2,
  "filters": {
    "module_ids": [],
    "file_paths": [],
    "symbol_kinds": ["function"],
    "relation_kinds": ["calls"],
    "confidence": ["resolved", "heuristic"],
    "search": null
  },
  "max_nodes": 500,
  "path_target": null,
  "continuation": null
}
```

`protocol_version` 必须为 `1`；`project_id` 必须为正数；`depth` 为 `0..=8`；`max_nodes` 为 `1..=500`。节点 ID 使用 `sym:<16 位十六进制>`、`file:<路径>`、`module:<名称>`、`entry:<名称>`、`group:<名称>` 或 `unresolved:<稳定哈希>`。继续请求使用 `v1:<无符号偏移>`。

字段名在 Rust/CLI/MCP JSON 中使用 snake_case。MCP 的输入字段使用 camelCase，并在边界转换为这里的字段名；MCP 不会静默修改 `depth` 或 `maxNodes`。

## Five graph kinds

下列请求只改变 `graph_kind`，其余字段可按需要填写。它们分别对应索引中的唯一事实来源：

| `graph_kind` | 节点与边来源 | 常见边类型 |
| --- | --- | --- |
| `module_dependencies` | `module_relations` | `include` |
| `function_calls` | `symbol_relations` 中 `relation_kind = calls` | `calls` |
| `class_relations` | `symbol_relations` 中 `inherits/implements/contains/uses` | `inherits`, `implements`, `contains`, `uses` |
| `file_includes` | `includes` | `includes` |
| `entry_paths` | `calls`，无根时使用 `entry_points` | `calls` |

完整的最小请求示例：

```json
{ "protocol_version": 1, "project_id": 7, "graph_kind": "module_dependencies", "root_node_ids": [], "direction": "both", "depth": 1, "filters": {}, "max_nodes": 500, "path_target": null, "continuation": null }
```

```json
{ "protocol_version": 1, "project_id": 7, "graph_kind": "function_calls", "root_node_ids": ["sym:0123456789abcdef"], "direction": "outgoing", "depth": 2, "filters": {}, "max_nodes": 500, "path_target": null, "continuation": null }
```

```json
{ "protocol_version": 1, "project_id": 7, "graph_kind": "class_relations", "root_node_ids": ["sym:fedcba9876543210"], "direction": "both", "depth": 2, "filters": {}, "max_nodes": 500, "path_target": null, "continuation": null }
```

```json
{ "protocol_version": 1, "project_id": 7, "graph_kind": "file_includes", "root_node_ids": ["file:src/main.cpp"], "direction": "outgoing", "depth": 2, "filters": {}, "max_nodes": 500, "path_target": null, "continuation": null }
```

```json
{ "protocol_version": 1, "project_id": 7, "graph_kind": "entry_paths", "root_node_ids": ["sym:0123456789abcdef"], "direction": "outgoing", "depth": 8, "filters": {}, "max_nodes": 500, "path_target": "sym:fedcba9876543210", "continuation": null }
```

## Response

每种图都返回同一完整结构；节点和边数组按稳定 ID 排序，便于增量渲染和分页合并。下面是五类图共用的完整字段示例（`graph_kind` 随请求改变）：

```json
{
  "protocol_version": 1,
  "project_id": 7,
  "graph_kind": "function_calls",
  "nodes": [
    { "id": "sym:0123456789abcdef", "kind": "function", "label": "main", "qualified_name": "app::main", "file": "src/main.cpp", "line": 10, "module_id": "module:src", "confidence": "heuristic", "ambiguous": false, "metadata": {} },
    { "id": "unresolved:0123456789abcdef", "kind": "unresolved", "label": "missing", "qualified_name": "missing", "file": "src/main.cpp", "line": 20, "module_id": "module:src", "confidence": "unresolved", "ambiguous": false, "metadata": {} }
  ],
  "edges": [
    { "id": "edge:0123456789abcdef", "source": "sym:0123456789abcdef", "target": "unresolved:0123456789abcdef", "kind": "calls", "weight": 1, "confidence": "unresolved", "ambiguous": false, "evidence_ids": ["ev:0123456789abcdef"] }
  ],
  "groups": [],
  "evidence": [
    { "id": "ev:0123456789abcdef", "edge_id": "edge:0123456789abcdef", "file": "src/main.cpp", "line": 20, "detail": "calls missing", "confidence": "unresolved" }
  ],
  "confidence": { "overall": "unresolved", "resolved": 0, "hybrid": 0, "heuristic": 0, "ambiguous": 0, "unresolved": 1 },
  "statistics": { "original_node_count": 2, "returned_node_count": 2, "original_edge_count": 1, "returned_edge_count": 1, "cycle_count": 0, "unresolved_edge_count": 1, "ambiguous_edge_count": 0, "central_node_ids": ["sym:0123456789abcdef", "unresolved:0123456789abcdef"], "shortest_path_node_ids": [] },
  "truncated": false,
  "truncation_reason": null,
  "continuation": null
}
```

`groups` 用于循环依赖等可视化分组，`evidence` 将边连接回文件和行号。无法解析的目标保留原始名称，并使用稳定的 `unresolved:` 占位节点；歧义边的 `ambiguous` 为 true，候选目标保存在节点 metadata/索引事实中，不能被 UI 当成确定语义。

## Traversal and analysis

- 有根查询按 `direction` 和 `depth` 做确定性 BFS；`depth = 0` 只返回根节点。
- 无根查询选择度数最高的最多 10 个节点并展开一跳；`entry_paths` 优先使用扫描得到的入口点。
- 筛选在遍历、统计和分页前应用：模块、文件、符号种类、关系种类、置信度和大小写折叠搜索均可组合。
- `path_target` 存在时返回根到目标的最短路径；目标不属于当前图时返回验证错误。
- `cycle_count` 使用强连通分量统计；循环分量以 `groups[].kind = "cycle"` 返回。
- `central_node_ids` 按入度加出度降序、稳定 ID 升序排列，最多 10 个。
- `original_*` 是筛选后的完整图统计；分页只返回当前页两端都出现的边。

当节点数超过 `max_nodes` 时，响应设置 `truncated = true`、`truncation_reason = "node_limit"`，并给出 `continuation = "v1:<下一偏移>"`。下一请求复用同一请求并替换 continuation；分页不会改变节点排序或筛选结果。

## Stable facts and compatibility

扫描器 schema 3 写入稳定符号键、限定名、父作用域、调用者归属、候选目标、类型关系、入口点及证据置信度。旧 schema 2 JSON/数据库会在导入或打开时补齐稳定键和兼容字段；无法恢复的语义使用 `legacy-json/2` 和 heuristic 置信度。索引写入在单事务中完成，失败不会切换当前完整 generation。

图布局以 `(project_id, graph_kind, view_key)` 为作用域保存，坐标按稳定节点 ID 存储，最多 500 个有限坐标；保存同一 view 会原子替换，不影响其他图或 view。

## CLI and MCP

CLI 从 stdin 读取完整 JSON，按项目根目录解析本地 project ID，并只向 stdout 输出响应 JSON：

```powershell
cppinsight-workbench graph --root D:\repos\demo --request graph-request.json
cppinsight-workbench graph --root D:\repos\demo --request - < graph-request.json
```

MCP 工具名为 `cppinsight_graph`，输入使用 `projectPath`, `graphKind`, `rootNodeIds`, `maxNodes` 等 camelCase 字段，内部通过 CLI stdin 复用同一协议。图数据保留在本地；只有 `cppinsight_ask` 才会按用户设置发送 AI 请求。
