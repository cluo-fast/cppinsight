param(
    [Parameter(Mandatory = $true)]
    [string]$ProjectRoot,
    [string]$WorkbenchPath = ".\desktop\src-tauri\target\release\cppinsight-workbench.exe",
    [int]$Iterations = 3,
    [string]$OutputPath = ""
)

$ErrorActionPreference = "Stop"
if ($Iterations -lt 1) { throw "Iterations must be positive" }
if (-not (Test-Path -LiteralPath $WorkbenchPath)) {
    throw "Workbench binary not found: $WorkbenchPath"
}
if (-not (Test-Path -LiteralPath $ProjectRoot -PathType Container)) {
    throw "Project root not found: $ProjectRoot"
}

$resolvedProjectRoot = (Resolve-Path -LiteralPath $ProjectRoot).Path
$resolvedWorkbenchPath = (Resolve-Path -LiteralPath $WorkbenchPath).Path
$projectName = (Get-Item -LiteralPath $resolvedProjectRoot).Name
$workbenchName = (Get-Item -LiteralPath $resolvedWorkbenchPath).Name

function Quote-ProcessArgument([string]$Value) {
    return '"' + $Value.Replace('"', '\"') + '"'
}

function Invoke-GraphQuery([string]$RequestJson) {
    $start = [System.Diagnostics.Stopwatch]::StartNew()
    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $resolvedWorkbenchPath
    $startInfo.Arguments = "graph --root $(Quote-ProcessArgument $resolvedProjectRoot) --request -"
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardInput = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true

    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    if (-not $process.Start()) {
        throw "Failed to start Workbench: $resolvedWorkbenchPath"
    }
    $process.StandardInput.Write($RequestJson)
    $process.StandardInput.Close()
    $stdout = $process.StandardOutput.ReadToEnd()
    $stderr = $process.StandardError.ReadToEnd()
    $process.WaitForExit()
    $start.Stop()

    [pscustomobject]@{
        stdout = $stdout
        stderr = $stderr
        exit_code = $process.ExitCode
        elapsed_ms = [math]::Round($start.Elapsed.TotalMilliseconds, 2)
        peak_working_set_mb = [math]::Round($process.PeakWorkingSet64 / 1MB, 2)
    }
}

function Get-Median([double[]]$Values) {
    $sorted = @($Values | Sort-Object)
    $middle = [int][math]::Floor($sorted.Count / 2)
    if ($sorted.Count % 2 -eq 1) {
        return [double]$sorted[$middle]
    }
    return [math]::Round(([double]$sorted[$middle - 1] + [double]$sorted[$middle]) / 2, 2)
}

$kinds = @(
    "module_dependencies",
    "function_calls",
    "class_relations",
    "file_includes",
    "entry_paths"
)
$results = @()

foreach ($kind in $kinds) {
    $request = [ordered]@{
        protocol_version = 1
        project_id = 0
        graph_kind = $kind
        root_node_ids = @()
        direction = "both"
        depth = 2
        filters = [ordered]@{}
        max_nodes = 500
        path_target = $null
        continuation = $null
    }
    $requestJson = $request | ConvertTo-Json -Depth 8 -Compress
    $samples = @()
    $memorySamples = @()
    $last = $null

    for ($iteration = 0; $iteration -lt $Iterations; $iteration++) {
        $invocation = Invoke-GraphQuery $requestJson
        if ($invocation.exit_code -ne 0) {
            throw "Graph query failed for ${kind}: $($invocation.stderr.Trim())"
        }
        try {
            $last = $invocation.stdout | ConvertFrom-Json
        }
        catch {
            throw "Graph query returned invalid JSON for ${kind}: $($_.Exception.Message)"
        }
        if ($null -eq $last.statistics -or $null -eq $last.truncated) {
            throw "Graph query response is missing statistics/truncated for ${kind}"
        }
        $samples += [double]$invocation.elapsed_ms
        $memorySamples += [double]$invocation.peak_working_set_mb
    }

    $results += [ordered]@{
        graph_kind = $kind
        iterations = $Iterations
        samples_ms = $samples
        median_ms = Get-Median $samples
        peak_working_set_mb_samples = $memorySamples
        peak_working_set_mb = [math]::Round(($memorySamples | Measure-Object -Maximum).Maximum, 2)
        original_nodes = $last.statistics.original_node_count
        original_edges = $last.statistics.original_edge_count
        returned_nodes = $last.statistics.returned_node_count
        returned_edges = $last.statistics.returned_edge_count
        truncated = [bool]$last.truncated
        truncation_reason = $last.truncation_reason
    }
}

$report = [ordered]@{
    generated_at = (Get-Date).ToUniversalTime().ToString("o")
    project_name = $projectName
    workbench_name = $workbenchName
    results = $results
}
$json = $report | ConvertTo-Json -Depth 8
if ($OutputPath) {
    $json | Set-Content -LiteralPath $OutputPath -Encoding UTF8
    Write-Output "Wrote $OutputPath"
} else {
    Write-Output $json
}
