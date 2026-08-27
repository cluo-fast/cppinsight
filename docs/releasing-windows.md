# Windows 发布指南

CppInsight AI 的 Windows 安装包由 Windows runner 原生构建。仓库可以保证源码、依赖锁文件、测试、安装包和校验和的可复现流程，但不能凭空产生可信的发布者身份：正式公开分发前，项目所有者仍需向可信 CA 或 Microsoft Azure Artifact Signing 申请代码签名能力。

## 本地无签名构建

在 Windows 10/11、Visual Studio C++ Build Tools、CMake、Rust stable、Node.js 22 和 pnpm 11.19.0 已安装的环境中，从仓库根目录运行：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/package-app.ps1
```

脚本会构建并测试 C++ 扫描器、复制 `cppinsight.exe`、按 `desktop/pnpm-lock.yaml` 安装依赖、验证发布配置，并生成 NSIS 与 MSI 安装包。无签名安装包可用于内部测试，但从浏览器下载后可能触发 Windows SmartScreen 警告。

## GitHub Actions

`.github/workflows/windows-ci.yml` 在 PR 和 `main` 分支提交上构建无签名安装包，并上传：

- MSI 安装包；
- NSIS `-setup.exe` 安装包；
- `SHA256SUMS.txt`。

首个推送后的 Windows CI 才是 MSI/NSIS 构建成功的权威证据；macOS 或 Linux 本地构建不能替代这一步。

## Authenticode 签名

将 PFX 证书和密码配置为 GitHub Actions repository secrets：

- `WINDOWS_CERTIFICATE`：PFX 文件的纯 Base64 内容；
- `WINDOWS_CERTIFICATE_PASSWORD`：PFX 导出密码。

在 Actions 页面手动运行 `Windows desktop`，勾选 `sign`。工作流只在该手动任务中把证书导入 runner 的当前用户证书库，动态生成不入库的 Tauri 签名配置，完成后由临时 runner 销毁相关材料。日志中不得输出上述 secret。

正式签名前需确认：证书主题是预期发布主体、证书允许代码签名、时间戳服务可访问，并在下载后的安装包上执行：

```powershell
Get-AuthenticodeSignature .\CppInsight-AI-setup.exe | Format-List
```

状态必须为 `Valid`，签名主体必须与发行方一致。

## 更新器签名不是 Authenticode

`TAURI_SIGNING_PRIVATE_KEY` 与 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 预留给 Tauri 更新器制品签名。它们证明更新清单/制品由项目发布，不代表 Windows 发布者身份，也不能替代 PFX/Azure Authenticode 签名。当前版本尚未启用自动更新器；在更新端点、密钥轮换和回滚策略完成前，不应对外宣称支持自动更新。

## 校验、发布与回滚

发布前：

1. 下载 Actions artifact，在干净的 Windows 虚拟机验证安装、启动、卸载和重新安装。
2. 在 artifact 根目录执行 `Get-FileHash -Algorithm SHA256`，与 `SHA256SUMS.txt` 逐项比对。
3. 对签名发布额外验证 Authenticode 状态和签名主体。
4. 使用不含客户源码的测试仓库验证扫描、索引、设置和脱敏诊断导出。
5. 记录工作流 URL、提交 SHA、应用版本、证书主体和安装包哈希。

若发现严重问题，立即撤下对应下载入口，保留有问题制品及哈希用于定位，恢复上一版已验证安装包，并发布明确的受影响版本范围。不要用相同版本号覆盖旧制品；修复后递增版本重新构建和签名。

证书泄露时应先在 CA/Azure 撤销凭据并删除 GitHub secrets，再停止所有发布任务；更新器私钥泄露则需要独立执行更新信任链轮换。两类密钥的响应流程不可混用。
