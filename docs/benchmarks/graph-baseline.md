# 图查询基准与公开测试基线

## 目标

公开测试版参考目标是：默认子图不超过 500 个可见节点，首次出图不超过 2 秒；超过上限必须返回原始规模、明确的 `node_limit` 截断原因和可继续使用的 `v1:<offset>` token。性能数字必须在 Windows 参考机和真实基准仓库上重复测量，不能用隐藏关系或降低准确性换取速度。

## 可重复运行

在 Windows 构建 Release Workbench 后，从仓库根目录执行：

```powershell
.\scripts\benchmark-graph.ps1 `
  -ProjectRoot D:\repos\benchmark-project `
  -WorkbenchPath .\desktop\src-tauri\target\release\cppinsight-workbench.exe `
  -Iterations 5 `
  -OutputPath .\benchmark-graph.json
```

脚本对五类图各执行多次查询，记录耗时样本、准确的 median、查询进程峰值内存、完整/返回节点与边数、截断原因。请求通过 stdin 传递，避免路径或筛选值影响命令行解析；输出只保留项目名和 Workbench 文件名，不写入绝对路径。`benchmark-graph.json` 只用于本地或 CI 产物，不应提交客户源码或索引数据库。

生成结果后运行结构和隐私校验：

```powershell
node scripts/verify-benchmark-result.mjs .\benchmark-graph.json
```

校验器要求结果完整覆盖五类图、median 与样本一致、返回规模不超过原始规模，并拒绝项目路径或其他绝对路径泄露。Windows CI 会对内置示例执行一次三轮基准烟测；它只验证基准流水线本身，不替代真实 10 项目准确率和容量基准。

候选项目清单位于 [`corpus.json`](./corpus.json)，目前固定 10 个不同规模和构建体系的 C/C++ 项目。每次正式基准前必须把 `revision` 固定为 commit SHA，复核许可证，并补充人工核验的 `accuracy_cases`；校验器只负责保证清单结构完整，不把候选清单误报为已完成的准确率基准。

## 当前可验证证据

- Rust 集成测试覆盖 620 节点链的 500 节点分页、确定性 continuation、方向/深度、筛选、最短路径、歧义证据和循环分量。
- 内置 `examples/sample-cpp` 本地端到端烟测（macOS、启发式模式）扫描 5 个文件、11 个符号和 5 条调用线索；五类图查询均返回有效 Protocol v1 响应：模块依赖 2 节点/1 边，函数调用 10 节点/5 边，类关系 10 节点/2 边，文件包含 11 节点/10 边，入口路径 4 节点/3 边。
- 同一示例生成的接手摘要包含 CMake 构建上下文、`main` 入口、`main → sample::Engine::run → sample::Worker::process` 证据链和脱敏相对路径；该结果只证明产品链路可用，不替代 Windows 性能或真实项目准确率基准。
- 同一示例用 CMake 生成 3 条 `compile_commands.json` 后以混合模式复测：扫描模式为 `hybrid`，入口 `main` 为 `precise`，5 条调用线索均带 `precise` 置信度；五类图查询仍返回 Protocol v1 响应。该结果只证明编译数据库增强链路可用，不替代目标项目基准。
- 本地 C++ CTest、Rust 全量测试、Clippy `-D warnings`、TypeScript/Vite 构建和 release 配置验证均可重复通过。
- 真实 Windows 安装、2M 行/20k 文件容量、峰值内存、首次出图 2 秒和 10 个开源项目准确率仍必须由 Windows CI/基准机补测；在证据产生前不得标记为正式公开测试版。
