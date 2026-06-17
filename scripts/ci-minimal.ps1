# Minimal CI gate for pre-push validation
# Runs: cargo check, cargo test --no-run, pnpm typecheck
# Usage: pwsh scripts/ci-minimal.ps1
param([switch]$SkipFrontend)

$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location $projectRoot

$failed = $false
$start = Get-Date

Write-Host "=== CI Gate (minimal) ===" -ForegroundColor Cyan
Write-Host ""

# ── Rust: cargo check ──
Write-Host "[1/3] cargo check..." -ForegroundColor Yellow
$sw = [System.Diagnostics.Stopwatch]::StartNew()
$output = cargo check 2>&1
$sw.Stop()
if ($LASTEXITCODE -ne 0) {
    Write-Host "  FAILED (${sw.Elapsed.TotalSeconds}s)" -ForegroundColor Red
    $output | Select-String "error" | ForEach-Object { Write-Host "  $_" -ForegroundColor Red }
    $failed = $true
} else {
    $warnings = ($output | Select-String "warning:" | Measure-Object).Count
    Write-Host "  OK (${sw.Elapsed.TotalSeconds}s, $warnings warnings)" -ForegroundColor Green
}

# ── Rust: cargo test --no-run ──
Write-Host "[2/3] cargo test --no-run..." -ForegroundColor Yellow
$sw = [System.Diagnostics.Stopwatch]::StartNew()
$output = cargo test --no-run 2>&1
$sw.Stop()
if ($LASTEXITCODE -ne 0) {
    Write-Host "  FAILED (${sw.Elapsed.TotalSeconds}s)" -ForegroundColor Red
    $output | Select-String "error" | ForEach-Object { Write-Host "  $_" -ForegroundColor Red }
    $failed = $true
} else {
    Write-Host "  OK (${sw.Elapsed.TotalSeconds}s)" -ForegroundColor Green
}

# ── Frontend: pnpm typecheck ──
if (-not $SkipFrontend) {
    Write-Host "[3/3] pnpm typecheck..." -ForegroundColor Yellow
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $output = pnpm --filter dadou-app exec tsc --noEmit 2>&1
    $sw.Stop()
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  FAILED (${sw.Elapsed.TotalSeconds}s)" -ForegroundColor Red
        $output | Select-Object -Last 10 | ForEach-Object { Write-Host "  $_" -ForegroundColor Red }
        $failed = $true
    } else {
        Write-Host "  OK (${sw.Elapsed.TotalSeconds}s)" -ForegroundColor Green
    }
}

$elapsed = (Get-Date) - $start
Write-Host ""
if ($failed) {
    Write-Host "=== CI Gate: FAILED (${elapsed.TotalSeconds}s) ===" -ForegroundColor Red
    exit 1
} else {
    Write-Host "=== CI Gate: PASSED (${elapsed.TotalSeconds}s) ===" -ForegroundColor Green
    exit 0
}
