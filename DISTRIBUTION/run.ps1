# DADOU v1.0.0 — Lancement portable Windows (PowerShell)
# Exécutez ce script pour lancer DADOU.
# Le workspace (config, données, logs) est stocké dans le dossier
# "workspace\" à côté de dadou-core.exe.

$env:OPENHUMAN_WORKSPACE = $PSScriptRoot

Write-Host ""
Write-Host "╔══════════════════════════════════════════════╗"
Write-Host "║             DADOU v1.0.0                     ║"
Write-Host "║   Assistant IA autonome — mode hors-ligne     ║"
Write-Host "╚══════════════════════════════════════════════╝"
Write-Host ""
Write-Host "Workspace : $env:OPENHUMAN_WORKSPACE"
Write-Host "Interface : http://127.0.0.1:7788"
Write-Host "Dashboard : http://127.0.0.1:7790"
Write-Host ""
Write-Host "Appuyez sur Ctrl+C pour quitter."
Write-Host ""

& "$PSScriptRoot\dadou-core.exe" serve
