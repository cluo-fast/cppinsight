[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$BundleRoot,
    [string]$ProductName = "CppInsight AI",
    [string]$ExecutableName = "cppinsight-ai.exe",
    [int]$StartupTimeoutSeconds = 15
)

$ErrorActionPreference = "Stop"

function Assert-ExitCode([System.Diagnostics.Process]$Process, [string]$Action) {
    if ($Process.ExitCode -ne 0 -and $Process.ExitCode -ne 3010) {
        throw "$Action failed with exit code $($Process.ExitCode)"
    }
}

function Invoke-Installer([string]$FilePath, [string[]]$Arguments, [string]$Action) {
    $argumentString = ($Arguments | ForEach-Object {
        $argument = [string]$_
        if ($argument -match '[\s"]') {
            '"' + $argument.Replace('"', '\"') + '"'
        } else {
            $argument
        }
    }) -join ' '
    $process = Start-Process -FilePath $FilePath -ArgumentList $argumentString -Wait -PassThru -WindowStyle Hidden
    Assert-ExitCode $process $Action
}

function Find-InstalledExecutable([string]$Name, [string]$BinaryName) {
    $registryRoots = @(
        "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*",
        "HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*",
        "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*"
    )
    $records = Get-ItemProperty -Path $registryRoots -ErrorAction SilentlyContinue |
        Where-Object { $_.DisplayName -eq $Name }
    foreach ($record in $records) {
        if (-not [string]::IsNullOrWhiteSpace($record.InstallLocation)) {
            $candidate = Join-Path $record.InstallLocation $BinaryName
            if (Test-Path -LiteralPath $candidate) {
                return $candidate
            }
        }
    }
    $fallbacks = @(
        (Join-Path $env:ProgramFiles $Name),
        (Join-Path $env:LOCALAPPDATA "Programs\$Name")
    )
    foreach ($directory in $fallbacks) {
        $candidate = Join-Path $directory $BinaryName
        if (Test-Path -LiteralPath $candidate) {
            return $candidate
        }
    }
    return $null
}

function Assert-AppStarts([string]$Executable, [string]$Flavor) {
    $process = Start-Process -FilePath $Executable -PassThru
    try {
        $deadline = (Get-Date).AddSeconds($StartupTimeoutSeconds)
        do {
            Start-Sleep -Milliseconds 250
            if ($process.HasExited) {
                throw "$Flavor application exited during startup with code $($process.ExitCode)"
            }
        } while ((Get-Date) -lt $deadline)
    }
    finally {
        if (-not $process.HasExited) {
            Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
            [void]$process.WaitForExit(5000)
        }
    }
}

$resolvedBundleRoot = (Resolve-Path -LiteralPath $BundleRoot).Path
$msi = Get-ChildItem -LiteralPath $resolvedBundleRoot -Recurse -Filter "*.msi" -File | Select-Object -First 1
$nsis = Get-ChildItem -LiteralPath $resolvedBundleRoot -Recurse -Filter "*-setup.exe" -File | Select-Object -First 1
if ($null -eq $msi -or $null -eq $nsis) {
    throw "Expected one MSI and one NSIS installer under $resolvedBundleRoot"
}

Write-Host "Smoke testing MSI: $($msi.Name)"
Invoke-Installer "msiexec.exe" @("/i", $msi.FullName, "/qn", "/norestart") "MSI install"
$msiExecutable = Find-InstalledExecutable $ProductName $ExecutableName
if ([string]::IsNullOrWhiteSpace($msiExecutable)) {
    throw "MSI install did not register an executable for $ProductName"
}
Assert-AppStarts $msiExecutable "MSI"
Invoke-Installer "msiexec.exe" @("/x", $msi.FullName, "/qn", "/norestart") "MSI uninstall"
if (Test-Path -LiteralPath $msiExecutable) {
    throw "MSI uninstall left the application executable behind: $msiExecutable"
}
Invoke-Installer "msiexec.exe" @("/i", $msi.FullName, "/qn", "/norestart") "MSI reinstall"
$msiExecutable = Find-InstalledExecutable $ProductName $ExecutableName
if ([string]::IsNullOrWhiteSpace($msiExecutable)) {
    throw "MSI reinstall did not register an executable for $ProductName"
}
Assert-AppStarts $msiExecutable "MSI reinstall"
Invoke-Installer "msiexec.exe" @("/x", $msi.FullName, "/qn", "/norestart") "MSI final uninstall"
if (Test-Path -LiteralPath $msiExecutable) {
    throw "MSI final uninstall left the application executable behind: $msiExecutable"
}

$nsisInstallRoot = Join-Path $env:RUNNER_TEMP "cppinsight-nsis-smoke"
if (Test-Path -LiteralPath $nsisInstallRoot) {
    Remove-Item -LiteralPath $nsisInstallRoot -Recurse -Force
}
New-Item -ItemType Directory -Path $nsisInstallRoot -Force | Out-Null
Write-Host "Smoke testing NSIS: $($nsis.Name)"
Invoke-Installer $nsis.FullName @("/S", "/D=$nsisInstallRoot") "NSIS install"
$nsisExecutable = Join-Path $nsisInstallRoot $ExecutableName
if (-not (Test-Path -LiteralPath $nsisExecutable)) {
    throw "NSIS install did not produce the application executable: $nsisExecutable"
}
Assert-AppStarts $nsisExecutable "NSIS"
$uninstaller = Join-Path $nsisInstallRoot "uninstall.exe"
if (-not (Test-Path -LiteralPath $uninstaller)) {
    throw "NSIS install did not produce uninstall.exe"
}
Invoke-Installer $uninstaller @("/S") "NSIS uninstall"
if (Test-Path -LiteralPath $nsisInstallRoot) {
    throw "NSIS uninstall left the installation directory behind: $nsisInstallRoot"
}
New-Item -ItemType Directory -Path $nsisInstallRoot -Force | Out-Null
Invoke-Installer $nsis.FullName @("/S", "/D=$nsisInstallRoot") "NSIS reinstall"
$nsisExecutable = Join-Path $nsisInstallRoot $ExecutableName
if (-not (Test-Path -LiteralPath $nsisExecutable)) {
    throw "NSIS reinstall did not produce the application executable: $nsisExecutable"
}
Assert-AppStarts $nsisExecutable "NSIS reinstall"
$uninstaller = Join-Path $nsisInstallRoot "uninstall.exe"
Invoke-Installer $uninstaller @("/S") "NSIS final uninstall"
if (Test-Path -LiteralPath $nsisInstallRoot) {
    throw "NSIS final uninstall left the installation directory behind: $nsisInstallRoot"
}

Write-Host "Windows installer smoke test passed for MSI and NSIS."
