# Windows 接续清单（2026-09-15）

## 目标与分支

继续把 CppInsight AI 做成免费、可公开测试的 Windows 产品，复杂关系图是首发前置条件，暂不直接变现。先修复和验证，再扩展功能。用户已授权持续实施及推送到 `https://github.com/cluo-fast/cppinsight.git` 的 `codex/graph-workbench` 分支；不要擅自合并 main、公开私有仓库或发布未验证安装包。

本轮在该分支的 `51b334d` 基线上完成。Mac 开始收尾时 fetch 后本地/远端差异为 0/0；本清单随新功能一同提交，接续时以远端该分支最新提交为准，不固定回退到基线。Mac 主目录是另一份 main worktree，不是本轮功能目录；Windows 可在一个仓库目录直接切分支，不必复制 Mac 的 `.worktrees` 布局。

先读取 `git status --short`、`git branch -vv`、`git worktree list`，保护未提交文件。干净仓库可执行：

```powershell
git fetch origin
git switch codex/graph-workbench
git pull --ff-only origin codex/graph-workbench
git log -1 --format=fuller
```

若分支尚不存在，用 `git switch --track origin/codex/graph-workbench`。若本地与远端分叉或切分支被未提交内容阻挡，先检查并保存，不用 reset --hard、强推或自动覆盖。

## 本轮完成内容

- 修改影响调查：反向遍历真实 calls 边，区分直接/间接调用者、索引入口候选、涉及文件和模块；保留一条最短证据链，展示推断/歧义和深度、截断限制。
- 源码证据预览：Tauri 命令校验项目作用域，只读 SQLite 索引 chunks；显示快照时间、目标行和上下文，限制长行/总量，HTML 作为纯文本。
- 图调查快捷入口、分页保留已有坐标、依据查询方向精确高亮相邻路径边（含 SVG 导出），显示累计已加载数量。
- 新增组件和算法测试、后端预览测试、跨平台调查端到端烟测，并接入 Windows CI。

## Mac 已验证的证据边界

2026-09-15，以下命令均成功退出：

| 验证 | 结果 |
| --- | --- |
| desktop 的 `pnpm run check` | 生产构建、发布结构校验、全部检查通过 |
| `test:investigation` | 5 个组件测试通过；API/Cytoscape 使用 mock，非原生 UI 实测 |
| 图快照 / 图状态及影响算法 | 5 / 8 项通过（后者包含 5 项影响算法测试） |
| HTML / runtime / AI / artifact / benchmark / corpus | 2 / 2 / 4 / 3 / 3 / 2 项通过 |
| Rust | 46 个库测试 + 2 个 CLI 测试 + 14 个图协议测试通过 |
| Rust 格式 / Clippy | fmt --check、clippy --all-targets -- -D warnings 通过 |
| C++ | 新临时目录 Release 构建、CTest 1/1 通过 |
| MCP | npm test 4/4 通过，包含 stdio 传输 |
| Tauri | `pnpm tauri build --debug --no-bundle` 通过；不是 Windows 安装包 |
| 调查端到端 | 真扫描器 → SQLite 索引 → 五类图 → Engine::run 上游 main/证据 → 报告通过 |

没有证明 Windows runner、MSI/NSIS、签名、原生图界面或真实大项目容量已通过。Mac 生成的 scanner 资源即使文件名为 cppinsight.exe 也是 Mac 二进制，已忽略，不可拿去 Windows 使用或提交。

## Windows 执行顺序

1. 阅读本清单、`docs/optimization-plan-2026-09-08.md`、`docs/public-beta-readiness.md`、`docs/releasing-windows.md`。核对最新 Actions 的提交 SHA 与当前分支一致。历史失败运行为 https://github.com/cluo-fast/cppinsight/actions/runs/34026079324 ，job 101467167787 停在 Test Rust backend；此前私有仓库日志接口 404、浏览器读取超时，确切原因未知。获取真实日志再修复，不猜测原因，不删除/跳过失败测试。
2. 安装/核对仓库约定的 Node.js 22、pnpm 11.19.0、Rust、Visual Studio C++ Build Tools、CMake。desktop 使用 `pnpm install --frozen-lockfile`，执行 `pnpm run check`、Rust fmt/Clippy；MCP 目录执行 `npm ci --ignore-scripts` 和 `npm test`。
3. 从根目录执行 `powershell -ExecutionPolicy Bypass -File scripts/package-app.ps1`。确认 CMake/CTest、前端/Rust、Release CLI、MSI/NSIS 各步骤的实际退出码，不能仅凭旧文件存在认定构建成功。
4. 从根目录执行新增烟测：

   ```powershell
   node --experimental-strip-types scripts/smoke-investigation.mjs --scanner desktop/src-tauri/resources/cppinsight.exe --workbench desktop/src-tauri/target/release/cppinsight-workbench.exe
   ```

5. 按 CI 中的制品整理方式，把本次 MSI、NSIS 放到独立平铺的制品目录，运行 `scripts/windows-installer-smoke.ps1 -BundleRoot <该目录>`。该脚本实际安装、启动、卸载、重装，需用干净 VM/runner；不要在装有用户工作版本的机器无提示卸载。核验 SHA256SUMS 和实际安装后的程序/扫描器，不以源码开发启动替代安装验证。
6. 在 Windows 原生界面验收：内置示例首次索引与报告、五类图、缩放/拖拽/布局/导出；选中 Engine::run 调查上游入口与直接/间接调用者；证据点击打开正确源码行；改变深度、快速切换项目/重新索引后无旧结果覆盖；取消索引保留旧完整数据、重启恢复、AI 关闭时仍可本地工作且不发送模型请求。记录截图、日志、提交 SHA。
7. 大图专门检查分页跨页关系是否丢失、累计节点上限及布局稳定性。本轮只修了坐标保留；既有分页协议按页筛边，不能把前端合并测试通过当成全量跨页边完整。先补失败回归再修复协议/合并逻辑。
8. 按既有基准清单落地真实项目 revision、授权来源、人工准确率核对样本，运行 `scripts/benchmark-graph.ps1` 及结果校验器，记录机器、耗时、内存、截断和准确率。示例烟测不代表大项目容量。再按优化计划推进模块聚合、第三方依赖筛选、调查历史/书签、Git 差异影响分析；这些尚未实现。
9. 更新 readiness 与发布说明，只勾选有证据的项目；记录结果、制品哈希和剩余障碍，提交并推送同一分支。签名身份/证书、版权主体/许可证确认、公众反馈/下载服务缺失时分别列明，继续不依赖它们的工作，不伪造证据或密钥，不擅自选择法律授权或开通付费服务。

## 当前不能宣称完成的事项

Windows 干净安装和原生 UI 验证、失败 CI 的复验、真实大项目性能/准确率、跨页边完整性、签名与授权、可供公众访问的反馈/下载入口。源码仓库私有，GitHub Issues 不能作为公众唯一反馈渠道。在公开测试门槛完成前，只称为内部测试构建。
