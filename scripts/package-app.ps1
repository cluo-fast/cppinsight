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
    pnpm run verify:bundle
    if ($LASTEXITCODE -ne 0) { throw "release configuration verification failed" }
    pnpm run check
    if ($LASTEXITCODE -ne 0) { throw "frontend or Rust verification failed" }
    pnpm tauri build
    if ($LASTEXITCODE -ne 0) { throw "Tauri bundle build failed" }
}
finally {
    Pop-Location
}

$BundleRoot = Join-Path $RepoRoot "desktop/src-tauri/target/release/bundle"
$Installers = Get-ChildItem -LiteralPath $BundleRoot -Recurse -File |
    Where-Object { $_.Extension -eq ".msi" -or $_.Name.EndsWith("-setup.exe") }
if ($Installers.Count -lt 2) {
    throw "Expected both MSI and NSIS installers under $BundleRoot"
}
$Checksums = foreach ($Installer in $Installers) {
    $Hash = (Get-FileHash -LiteralPath $Installer.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    $Relative = [IO.Path]::GetRelativePath($BundleRoot, $Installer.FullName)
    "$Hash  $Relative"
}
$ManifestPath = Join-Path $BundleRoot "SHA256SUMS.txt"
$Checksums | Set-Content -LiteralPath $ManifestPath
node (Join-Path $RepoRoot "scripts/verify-sha256-manifest.mjs") $ManifestPath
if ($LASTEXITCODE -ne 0) { throw "artifact hash manifest verification failed" }
Write-Host "CppInsight AI Windows installers are under $BundleRoot."
Write-Host "SHA-256 manifest written to $ManifestPath."
