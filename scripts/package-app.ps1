$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $PSScriptRoot
$BuildDir = Join-Path $RepoRoot "build-release"
$ResourceDir = Join-Path $RepoRoot "desktop/src-tauri/resources"

function Assert-Command([string]$Name, [string]$Hint) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw $Hint
    }
}

Assert-Command "cmake" "cmake not found. Install CMake 3.16 or newer."
Assert-Command "cargo" "cargo not found. Install Rust from https://rustup.rs."
Assert-Command "pnpm" "pnpm not found. Install pnpm 11.19 or newer."

cmake -S $RepoRoot -B $BuildDir -DCMAKE_BUILD_TYPE=Release
cmake --build $BuildDir --config Release
ctest --test-dir $BuildDir -C Release --output-on-failure

$ScannerCandidates = @(
    (Join-Path $BuildDir "Release/cppinsight.exe"),
    (Join-Path $BuildDir "cppinsight.exe")
)
$Scanner = $ScannerCandidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if (-not $Scanner) {
    throw "cppinsight.exe was not produced in $BuildDir"
}

New-Item -ItemType Directory -Force $ResourceDir | Out-Null
Copy-Item -LiteralPath $Scanner -Destination (Join-Path $ResourceDir "cppinsight.exe") -Force

Push-Location (Join-Path $RepoRoot "desktop")
try {
    pnpm install --frozen-lockfile
    if ($LASTEXITCODE -ne 0) { throw "pnpm install failed" }
    pnpm run verify:release
    if ($LASTEXITCODE -ne 0) { throw "release configuration verification failed" }
    pnpm run build
    if ($LASTEXITCODE -ne 0) { throw "frontend build failed" }
    pnpm tauri build
    if ($LASTEXITCODE -ne 0) { throw "Tauri bundle build failed" }
}
finally {
    Pop-Location
}

Write-Host "CppInsight AI Windows installers are under desktop/src-tauri/target/release/bundle."
