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

$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("cppinsight-graph-benchmark-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tempRoot | Out-Null
$kinds = @(
    "module_dependencies",
    "function_calls",
    "class_relations",
    "file_includes",
    "entry_paths"
)
$results = @()

try {
    foreach ($kind in $kinds) {
        $requestPath = Join-Path $tempRoot "$kind.json"
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
        $request | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $requestPath -Encoding UTF8
        $samples = @()
        $last = $null
        for ($iteration = 0; $iteration -lt $Iterations; $iteration++) {
            $responsePath = Join-Path $tempRoot "$kind-$iteration.json"
            $elapsed = Measure-Command {
                Get-Content -LiteralPath $requestPath -Raw |
                    & $WorkbenchPath graph --root $ProjectRoot --request - |
                    Set-Content -LiteralPath $responsePath -Encoding UTF8
            }
            if ($LASTEXITCODE -ne 0) { throw "Graph query failed for $kind" }
            $last = Get-Content -LiteralPath $responsePath -Raw | ConvertFrom-Json
            $samples += [math]::Round($elapsed.TotalMilliseconds, 2)
        }
        $results += [ordered]@{
            graph_kind = $kind
            iterations = $Iterations
            samples_ms = $samples
            median_ms = ($samples | Sort-Object)[[math]::Floor($samples.Count / 2)]
            original_nodes = $last.statistics.original_node_count
            original_edges = $last.statistics.original_edge_count
            truncated = $last.truncated
        }
    }

    $report = [ordered]@{
        generated_at = (Get-Date).ToUniversalTime().ToString("o")
        project_root = (Resolve-Path -LiteralPath $ProjectRoot).Path
        workbench = (Resolve-Path -LiteralPath $WorkbenchPath).Path
        results = $results
    }
    $json = $report | ConvertTo-Json -Depth 8
    if ($OutputPath) {
        $json | Set-Content -LiteralPath $OutputPath -Encoding UTF8
        Write-Output "Wrote $OutputPath"
    } else {
        Write-Output $json
    }
}
finally {
    if (Test-Path -LiteralPath $tempRoot) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
}
