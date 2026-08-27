# CppInsight AI Desktop

Windows 桌面端由 React/Vite 前端与 Tauri/Rust 后端组成，运行时调用一并打包的 C++ 扫描器。当前桌面端属于公开测试版前的工程阶段；五类复杂关系图仍是首发前置条件，不能把当前构建当作正式公开版本。

## 日常启动

从仓库根目录双击 `启动CppInsight.bat`，或运行：

```powershell
.\scripts\start-desktop.ps1
```

脚本会构建缺失的扫描器和桌面依赖，然后构建并启动 production 可执行文件。强制重建可设置 `CPPINSIGHT_REBUILD=1`。

## 前端与 Rust 验证

```powershell
cd desktop
pnpm install --frozen-lockfile
pnpm run verify:release
pnpm run build
cargo test --manifest-path src-tauri\Cargo.toml
```

`pnpm run check` 会依次运行发布结构检查、TypeScript/Vite 生产构建和 Rust 测试。C++ 的 CMake/CTest 验证仍需从仓库根目录单独运行。

`verify:release` 允许尚未生成的扫描器资源，适合本地源码检查；打包脚本和 Windows CI 使用严格的 `verify:bundle`，要求 `resources/cppinsight.exe` 已实际存在。

## 开发模式

先确保 `desktop/src-tauri/resources/cppinsight.exe` 已生成，再运行：

```powershell
cd desktop
pnpm tauri dev
```

通常建议直接使用根目录的 `scripts/start-desktop.ps1`，因为它会处理扫描器构建和资源复制。

## 打包

从仓库根目录运行：

```powershell
.\scripts\package-app.ps1
```

`scripts/package-app.ps1` 会构建并测试 C++ Release 扫描器，复制资源，按锁文件安装前端依赖，运行验证并调用 Tauri 生成 NSIS/MSI。Windows 签名、GitHub secrets、SHA-256 校验与回滚清单见 `docs/releasing-windows.md`。

macOS 上的 Rust/前端测试不能证明 Windows 安装包可用；`.github/workflows/windows-ci.yml` 的 Windows runner 是 MSI/NSIS 构建的权威验证环境。

## 安全约束

- Tauri 页面使用严格 CSP，能力清单只保留 core/event/dialog。
- API Key 由操作系统凭据库保存；前端只读取 `api_key_configured`。
- 空密钥输入保持原值，只有显式勾选才清除。
- 诊断 JSON 不包含源码、绝对项目路径、聊天内容和凭据，并拒绝覆盖已有文件。
- 不要把 `resources/cppinsight.exe`、PFX、私钥或动态签名配置提交到仓库。
