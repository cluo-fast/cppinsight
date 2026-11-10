# 许可证

整个 CppInsight AI 仓库以 Apache-2.0 授权，不再把核心和桌面拆成两套许可。

覆盖范围包括：

- `src/` 扫描器、`tests/`、扫描器 CLI
- `desktop/` 桌面应用（React/Vite 前端与 Tauri/Rust 后端）和 Workbench CLI
- `tools/mcp-server/` MCP 服务
- `examples/` 中的内置示例（第一方示例，不是第三方代码）
- 协议说明、基准说明和其他文档

授权文本是仓库根目录的 [LICENSE](../LICENSE)。版权声明见 [NOTICE](../NOTICE)。直接依赖及其许可证见 [THIRD_PARTY_NOTICES](../THIRD_PARTY_NOTICES)。

## 版权主体

仓库没有写明独立的法人版权人，因此 LICENSE 与 NOTICE 中的版权人写作 **CppInsight AI contributors**。这是仓库内的表述，不是法律意见。

如果这些提交实际上是职务作品，或者版权属于雇主或其他主体，所有者必须在把仓库改为公开之前确认自己有权以该名义按 Apache-2.0 发布。未确认之前，不要把本文件理解成雇主已经同意公开。

## 和旧文档的关系

较早的设计说明曾计划采用核心公开、桌面保留的拆分。那个计划已经停用。以本文件和根目录 LICENSE 为准。历史设计稿里的原文只作为当时的记录保留。

## 第三方组件

Cytoscape.js、Tauri、Rust crate、Node 依赖和（通过 `rusqlite` 的 `bundled` 特性打进二进制的）SQLite 仍使用各自的上游许可证。分发安装包时请带上 LICENSE、NOTICE 和 THIRD_PARTY_NOTICES。双重许可的组件可以在其声明的任一许可证下使用，本项目不代替上游做选择。

基准清单里的外部 C/C++ 项目不是本仓库的一部分。引用它们做准确率测试时，仍要单独遵守那些项目自己的许可证，不要把它们的源码提交进本仓库。

## 贡献

除非贡献者另有书面说明，发到本仓库的贡献按 Apache-2.0 接收。见 [CONTRIBUTING.md](../CONTRIBUTING.md)。

本文件说明仓库里的授权边界，不构成法律意见。
