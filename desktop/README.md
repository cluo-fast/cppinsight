# CppInsight AI Desktop

普通桌面应用入口：请从仓库根目录双击 `启动CppInsight.bat`。

## 日常使用

1. 双击 `启动CppInsight.bat`
2. 点「打开 C/C++ 项目」
3. 看总览，再提问 / 导出客户摘要

## 开发

```powershell
# 仓库根目录
.\scripts\start-desktop.ps1
```

强制开发模式（即使已有 release 包）：

```powershell
$env:CPPINSIGHT_FORCE_DEV = "1"
.\scripts\start-desktop.ps1
```

## 打包

```powershell
.\scripts\package-app.ps1
```

打包会把 `cppinsight.exe` 一并打进 `resources`，安装后无需再记 CLI。
