# CppInsight AI 公开测试准备清单

这份清单把源码已完成项与必须在目标环境实测的发布门槛分开。只有所有“发布门槛”都有证据，才可把安装包称为公开测试版。

## 已在当前工作区验证

- [x] C++ 扫描器稳定符号键、限定名、父作用域、调用归属、类型关系和入口点。
- [x] schema 2 兼容迁移、schema 3 图事实、完整 generation 事务回滚。
- [x] 统一 Graph Protocol v1：五类图、方向/深度/筛选、路径、循环、中心节点、证据和分页。
- [x] Tauri 异步 `query_graph`、布局保存/加载及稳定节点 ID 校验。
- [x] 桌面 GraphWorkbench：五类图切换、搜索/筛选、缩放、拖拽、路径目标、证据面板、分页和 SVG/PNG 导出。
- [x] Cytoscape.js 3.34.1 已锁定并由 Vite 生产构建打包；图节点颜色、路径高亮和 zoom 控件已接入实例。
- [x] 接手报告加入模块、包含、类型和入口图洞察；AI 关闭时本地报告和图查询仍可用。
- [x] CLI stdin 图查询与 MCP `cppinsight_graph` 协议转换代码；不静默截断参数。
- [x] C++ CTest、Rust 35 单测 + 14 图协议测试、Clippy、TypeScript/Vite 和 release verifier。
- [x] MCP SDK 安装后运行 `npm test` 通过（3/3），无 SDK 的核心契约测试也可独立运行。

## 目标环境发布门槛

- [ ] Windows runner 原生完成 Release 扫描器、Tauri MSI/NSIS 安装、启动、卸载、重装端到端测试。
- [ ] Windows artifact 生成 SHA-256 清单并在干净虚拟机核验。
- [ ] 正式发布包完成 Authenticode 签名并验证主体、时间戳和 SmartScreen 行为。
- [ ] 在 Windows 目标环境确认五类图的实际交互、缩放、布局和导出；缺包后备只用于源码开发环境。
- [ ] 在干净 Windows/CI runner 安装 MCP SDK 后运行传输测试，确认跨平台 stdio 行为。
- [ ] 在约定基准机运行 `scripts/benchmark-graph.ps1`，记录五类图延迟、内存、截断和准确率。
- [ ] 用不含客户源码的示例仓库完成首次安装、索引、报告、关系图、AI 关闭模式和脱敏诊断回归。

发布说明必须包含已知限制、性能实测、解析器启发式边界、隐私说明、artifact 哈希和回滚版本；未满足门槛时只能称为内部测试构建。
