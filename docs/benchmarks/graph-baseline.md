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

## 当前可验证证据

- Rust 集成测试覆盖 620 节点链的 500 节点分页、确定性 continuation、方向/深度、筛选、最短路径、歧义证据和循环分量。
- 本地 C++ CTest、Rust 全量测试、Clippy `-D warnings`、TypeScript/Vite 构建和 release 配置验证均可重复通过。
- 真实 Windows 安装、2M 行/20k 文件容量、峰值内存、首次出图 2 秒和 10 个开源项目准确率仍必须由 Windows CI/基准机补测；在证据产生前不得标记为正式公开测试版。
