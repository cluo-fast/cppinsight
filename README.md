# CppInsight AI

CppInsight AI 是面向陌生、遗留或缺少文档的 C/C++ 项目的本地接手工具。它把源码扫描、SQLite 索引、项目总览、符号关系查询和带引用的 AI 问答放进一个 Windows 桌面应用，目标产物是可复查的**项目接手报告**与复杂代码**关系图**，而不是通用聊天客户端。

## 当前状态

仓库已具备可测试的产品基础，但**尚未达到公开测试版发布条件**。

已经实现：

- C/C++ 文件盘点、代码/注释/空行统计、目录与扩展名分布；
- `compile_commands.json` 辅助的混合分析，以及基于文件时间戳的增量复用；
- 符号、`#include` 和函数调用线索提取；
- 项目本地 SQLite 索引、多项目与多会话恢复；
- 项目总览、热点文件、入口候选、调用者/被调用者和入口路径查询；
- 带源码引用的可选 AI 问答，以及客户摘要导出；
- 索引完成后自动生成本地接手报告，报告可直接跳转五类关系图；
- CLI、workbench CLI 和 stdio MCP 服务；
- Windows 打包基础、严格 CSP、操作系统凭据库和脱敏诊断导出。

关系图基础现在已包括统一 Graph Protocol v1、五类关系图工作台、可追溯证据、路径/循环/中心节点分析、布局保存和接手报告融合。协议、CLI/MCP 传输和图基准说明分别见 [Graph Protocol v1](docs/graph-protocol-v1.md)、[MCP 服务](tools/mcp-server/README.md) 和 [图查询基准](docs/benchmarks/graph-baseline.md)。

公开测试前仍必须完成的发布门槛：

- Windows runner 原生安装包、端到端安装验证、校验和与 Authenticode 签名；
- 目标环境中安装 Cytoscape.js 后的五类图交互与导出实测；
- MCP SDK 依赖可获取后的 Node 传输测试；
- 真实基准项目的延迟、内存和准确率记录。

GraphWorkbench 使用统一、版本化的图查询模型，目标渲染引擎为 Cytoscape.js，并在未安装依赖的源码开发环境提供确定性后备画布；支持筛选、搜索、渐进展开、路径高亮、证据检查和节点上限保护。在上述发布门槛和 Windows CI 实机结果完成前，不应把当前分支描述为公开测试版。

## Windows 桌面使用

首次启动可直接选择“体验内置示例”，无需准备客户源码即可查看扫描、接手报告和五类关系图；也可点击“打开项目”选择本地 C/C++ 目录。

开发仓库中的便捷入口：

```powershell
.\启动CppInsight.bat
```

也可以从源码启动：

```powershell
.\scripts\start-desktop.ps1
```

首次打开项目会扫描并写入 `<项目>/.cppinsight/index.sqlite`；以后打开优先复用本地索引。代码发生变化后可在界面中主动重新索引。

## 隐私与 AI 边界

- 扫描、统计、索引和关系查询默认在本机完成。
- 只有用户主动发起 AI 问答时，相关问题与检索出的少量代码上下文才会发送到用户配置的 OpenAI 或兼容端点。
- API Key 保存到 Windows Credential Manager（macOS 开发环境使用 Keychain），不会保存到 SQLite，也不会返回给前端页面。
- 设置中的诊断预览/导出只包含版本、平台、匿名项目 ID 和聚合计数，不包含源码、绝对路径、聊天内容或 API Key。
- 环境变量 `OPENAI_BASE_URL`、`OPENAI_API_KEY`、`OPENAI_MODEL` 优先于桌面设置。

## C++ 扫描器 CLI

```powershell
cmake -S . -B build -DBUILD_TESTING=ON
cmake --build build --config Release
ctest --test-dir build -C Release --output-on-failure

.\build\Release\cppinsight.exe scan D:\code\project --json scan.json --overview overview.md
.\build\Release\cppinsight.exe overview D:\code\project --output overview.md
.\build\Release\cppinsight.exe brief D:\code\project --output brief.md
```

需要更准确的编译上下文时可添加 `--compile-commands <file>`；增量扫描可添加 `--incremental [previous-json]`。

当前解析器仍包含启发式逻辑。宏、模板、条件编译、函数指针、动态分派及不完整编译数据库可能降低关系置信度，界面和报告必须保留这种不确定性，而不能把线索冒充完整语义事实。

## Workbench CLI 与 MCP

桌面建立索引后，可直接查询：

```powershell
cargo build --release --manifest-path desktop\src-tauri\Cargo.toml --bin cppinsight-workbench
.\desktop\src-tauri\target\release\cppinsight-workbench.exe inspect --root D:\code\project encryptFile
.\desktop\src-tauri\target\release\cppinsight-workbench.exe ask --root D:\code\project "入口在哪里？"
```

MCP 服务位于 `tools/mcp-server`，提供 `cppinsight_inspect`、`cppinsight_ask` 与 `cppinsight_graph`。配置示例见 [tools/mcp-server/README.md](tools/mcp-server/README.md)。

## 桌面开发、验证与发布

```powershell
cd desktop
pnpm install --frozen-lockfile
pnpm run check
```

完整 Windows 打包：

```powershell
.\scripts\package-app.ps1
```

安装包签名、校验和、CI 与回滚步骤见 [Windows 发布指南](docs/releasing-windows.md)。产品架构和首发关系图验收标准见 [公开测试版设计](docs/superpowers/specs/2026-08-27-public-beta-code-graph-design.md)。

公开测试准备状态见 [公开测试准备清单](docs/public-beta-readiness.md)。

## 反馈材料

公开测试反馈入口：[提交 Issue](https://github.com/cluo-fast/cppinsight/issues/new/choose)。报告问题时，请附上应用版本、复现步骤和设置页导出的脱敏诊断 JSON。不要上传客户源码、项目索引、真实路径、API Key 或含敏感内容的聊天记录。
