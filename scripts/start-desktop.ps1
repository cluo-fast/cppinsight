$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

$LogDir = Join-Path $Root "build"
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
$LogFile = Join-Path $LogDir "desktop-launch.log"

function Write-Log([string]$Message) {
    $line = "[{0}] {1}" -f (Get-Date -Format "yyyy-MM-dd HH:mm:ss"), $Message
    Write-Host $line
    Add-Content -Path $LogFile -Value $line -Encoding UTF8
}

function Ensure-PathTooling {
    $cargo = Join-Path $env:USERPROFILE ".cargo\bin"
    if (Test-Path $cargo) { $env:PATH = "$cargo;$env:PATH" }
    foreach ($path in @(
        (Join-Path $env:USERPROFILE "scoop\apps\nodejs\current"),
        "C:\nvm4w\nodejs",
        (Join-Path $env:ProgramFiles "nodejs")
    )) {
        if (Test-Path $path) { $env:PATH = "$path;$env:PATH" }
    }
}

function Assert-Command([string]$Name, [string]$Hint) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) { throw $Hint }
}

function Get-CliPath { Join-Path $Root "build\cppinsight.exe" }
function Get-AppExe { Join-Path $Root "desktop\src-tauri\target\release\cppinsight-ai.exe" }

function Ensure-CppInsightCli {
    $cli = Get-CliPath
    if (-not (Test-Path -LiteralPath $cli)) {
        Write-Log "Building C++ scanner (first run may take a while)..."
        & cmd /c "`"$(Join-Path $PSScriptRoot 'build-cpp.bat')`""
        if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $cli)) {
            throw "Failed to build cppinsight.exe"
        }
    }

    $resourceDir = Join-Path $Root "desktop\src-tauri\resources"
    New-Item -ItemType Directory -Force -Path $resourceDir | Out-Null
    Copy-Item -LiteralPath $cli -Destination (Join-Path $resourceDir "cppinsight.exe") -Force
    $env:CPPINSIGHT_BIN = $cli
    return $cli
}

function Ensure-NodeDeps {
    $desktop = Join-Path $Root "desktop"
    Push-Location $desktop
    try {
        if (-not (Test-Path -LiteralPath (Join-Path $desktop "node_modules"))) {
            Write-Log "Installing desktop dependencies via npm..."
            npm install --no-fund --no-audit | ForEach-Object { Write-Host $_ }
            if ($LASTEXITCODE -ne 0) { throw "npm install failed" }
        }
    } finally {
        Pop-Location
    }
}

function Ensure-ReleaseApp {
    $cli = Get-CliPath
    $appExe = Get-AppExe
    $releaseDir = Split-Path -Parent $appExe
    $stamp = Join-Path $releaseDir ".cppinsight-production"

    $needBuild = ($env:CPPINSIGHT_REBUILD -eq "1") -or -not (Test-Path -LiteralPath $stamp) -or -not (Test-Path -LiteralPath $appExe)
    if ($needBuild) {
        Write-Log "Building production desktop app via tauri build..."
        Get-Process -Name "cppinsight-ai" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
        Start-Sleep -Milliseconds 500

        Push-Location (Join-Path $Root "desktop")
        try {
            # Tauri/npm write progress to stderr; with $ErrorActionPreference=Stop,
            # piping 2>&1 turns those NativeCommandError records into terminating errors.
            # Run via cmd so stderr is plain text and only the exit code matters.
            $buildLog = Join-Path $LogDir "tauri-build.log"
            cmd /c "npx tauri build --no-bundle > `"$buildLog`" 2>&1"
            Get-Content -LiteralPath $buildLog -ErrorAction SilentlyContinue | ForEach-Object { Write-Host $_ }
            if ($LASTEXITCODE -ne 0) {
                throw "npx tauri build --no-bundle failed (exit=$LASTEXITCODE). See $buildLog"
            }
            if (-not (Test-Path -LiteralPath $appExe)) { throw "Desktop exe missing after build: $appExe" }
            Set-Content -Path $stamp -Value (Get-Date).ToString("o") -Encoding UTF8
        } finally {
            Pop-Location
        }
    }

    New-Item -ItemType Directory -Force -Path $releaseDir | Out-Null | Out-Null
    Copy-Item -LiteralPath $cli -Destination (Join-Path $releaseDir "cppinsight.exe") -Force
    Copy-Item -LiteralPath $cli -Destination (Join-Path $Root "desktop\src-tauri\resources\cppinsight.exe") -Force
    if (-not (Test-Path -LiteralPath $stamp)) {
        Set-Content -Path $stamp -Value (Get-Date).ToString("o") -Encoding UTF8
    }

    if (-not (Test-Path -LiteralPath $appExe)) {
        throw "Desktop app exe not found: $appExe"
    }

    return [string]$appExe
}

try {
    Set-Content -Path $LogFile -Value "" -Encoding UTF8
    Write-Log "CppInsight launch start"
    Ensure-PathTooling
    Assert-Command "node" "node not found. Install Node.js first."
    Assert-Command "npm" "npm not found. Install Node.js first."
    Assert-Command "cargo" "cargo not found. Install Rust from https://rustup.rs"

    Write-Log "[1/3] Prepare scanner"
    $cli = [string](Ensure-CppInsightCli)
    Write-Log ("CLI = {0}" -f $cli)

    Write-Log "[2/3] Prepare desktop dependencies"
    Ensure-NodeDeps

    Write-Log "[3/3] Build/start production app"
    $appExe = [string](Ensure-ReleaseApp)
    Write-Log ("APP = {0}" -f $appExe)

    Get-Process -Name "cppinsight-ai" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 400

    $workDir = Split-Path -Parent $appExe
    $env:CPPINSIGHT_BIN = Join-Path $workDir "cppinsight.exe"
    $proc = Start-Process -FilePath $appExe -WorkingDirectory $workDir -PassThru
    Start-Sleep -Seconds 2

    if ($null -eq $proc) { throw "Start-Process returned null" }
    if ($proc.HasExited) { throw ("App exited immediately, exit={0}" -f $proc.ExitCode) }

    $alive = Get-Process -Id $proc.Id -ErrorAction SilentlyContinue
    if ($null -eq $alive) { throw "App process disappeared after launch" }

    Write-Log ("App started, PID={0}, title={1}" -f $proc.Id, $alive.MainWindowTitle)
    Write-Host ""
    Write-Host "OK: CppInsight window should be open now."
    Write-Host ("Log: {0}" -f $LogFile)
    exit 0
}
catch {
    Write-Log ("ERROR: {0}" -f $_.Exception.Message)
    Write-Host ""
    Write-Host ("FAILED: {0}" -f $_.Exception.Message)
    Write-Host ("Log: {0}" -f $LogFile)
    exit 1
}