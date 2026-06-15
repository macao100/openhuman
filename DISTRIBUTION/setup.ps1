# DADOU v1.0 — Script d'installation automatisée
# Exécuter dans PowerShell (administrateur recommandé pour winget)
param(
    [switch]$SkipPrereqs,
    [switch]$SkipBuild,
    [switch]$InstallDocker,
    [string]$Workspace = "$env:USERPROFILE\DADOU"
)

$ErrorActionPreference = "Stop"
$ProjectDir = Split-Path -Parent $PSScriptRoot

Write-Host @"
========================================
  DADOU v1.0 — Installation automatisée
========================================
"@ -ForegroundColor Cyan

# ── Étape 1 : Prérequis ──────────────────────────────────────────────
if (-not $SkipPrereqs) {
    Write-Host "`n[1/4] Vérification des prérequis..." -ForegroundColor Yellow

    $Prereqs = @(
        @{Name="Rust";       Cmd="rustc --version";       MinVer="1.93"; Winget="Rustlang.Rustup"}
        @{Name="Node.js";    Cmd="node --version";         MinVer="24";   Winget="OpenJS.NodeJS.LTS"}
        @{Name="pnpm";       Cmd="pnpm --version";         MinVer="10";   Winget="pnpm.pnpm"}
        @{Name="CMake";      Cmd="cmake --version";        MinVer="3";    Winget="Kitware.CMake"}
        @{Name="LLVM";       Cmd="clang --version";        MinVer="18";   Winget="LLVM.LLVM"}
        @{Name="Git";        Cmd="git --version";          MinVer="2";    Winget="Git.Git"}
    )

    if ($InstallDocker) {
        $Prereqs += @{Name="Docker"; Cmd="docker --version"; MinVer="20"; Winget="Docker.DockerDesktop"}
    }

    $Missing = @()
    foreach ($p in $Prereqs) {
        try {
            $output = Invoke-Expression $p.Cmd 2>&1 | Out-String
            $version = $output -replace ".*?(\d+\.\d+).*", '$1'
            if ([version]$version -ge [version]$p.MinVer) {
                Write-Host "  ✅ $($p.Name) v$version" -ForegroundColor Green
            } else {
                Write-Host "  ⚠ $($p.Name) v$version (min $($p.MinVer))" -ForegroundColor Yellow
                $Missing += $p
            }
        } catch {
            Write-Host "  ❌ $($p.Name) non trouvé" -ForegroundColor Red
            $Missing += $p
        }
    }

    if ($Missing.Count -gt 0) {
        Write-Host "`n  Installation des outils manquants via winget..." -ForegroundColor Yellow
        foreach ($p in $Missing) {
            Write-Host "  → Installation de $($p.Name)..." -ForegroundColor Cyan
            winget install --id $p.Winget --silent --accept-package-agreements
        }
        Write-Host "  ✅ Outils installés. Redémarrez PowerShell pour appliquer les changements." -ForegroundColor Green
    }
}

# ── Étape 2 : Variables d'environnement ──────────────────────────────
Write-Host "`n[2/5] Configuration de l'environnement..." -ForegroundColor Yellow

$LlvmPath = "C:\Program Files\LLVM\bin"
if (Test-Path $LlvmPath) {
    [System.Environment]::SetEnvironmentVariable("LIBCLANG_PATH", $LlvmPath, "User")
    $env:LIBCLANG_PATH = $LlvmPath
    Write-Host "  ✅ LIBCLANG_PATH = $LlvmPath" -ForegroundColor Green
} else {
    Write-Host "  ❌ LLVM non trouvé dans $LlvmPath" -ForegroundColor Red
}

# Ajouter CMake et LLVM au PATH
$UserPath = [System.Environment]::GetEnvironmentVariable("PATH", "User")
$CmakePath = "C:\Program Files\CMake\bin"
if ((Test-Path $CmakePath) -and ($UserPath -notlike "*CMake*")) {
    [System.Environment]::SetEnvironmentVariable("PATH", "$UserPath;$CmakePath;$LlvmPath", "User")
    $env:PATH = "$env:PATH;$CmakePath;$LlvmPath"
    Write-Host "  ✅ CMake + LLVM ajoutés au PATH" -ForegroundColor Green
}

# ── Étape 2.5 : Dossier workspace ───────────────────────────────────
Write-Host "`n[2.5/5] Configuration du workspace..." -ForegroundColor Yellow
[System.Environment]::SetEnvironmentVariable("OPENHUMAN_WORKSPACE", $Workspace, "User")
$env:OPENHUMAN_WORKSPACE = $Workspace
New-Item -ItemType Directory -Force -Path $Workspace | Out-Null
Write-Host "  ✅ Workspace = $Workspace" -ForegroundColor Green

# Activer le mode offline par défaut (pas de backend cloud requis)
[System.Environment]::SetEnvironmentVariable("OPENHUMAN_OFFLINE_MODE", "true", "User")
$env:OPENHUMAN_OFFLINE_MODE = "true"
Write-Host "  ✅ Mode offline activé" -ForegroundColor Green
    Write-Host "  ✅ CMake + LLVM ajoutés au PATH" -ForegroundColor Green
}

# ── Étape 3 : Dépendances ────────────────────────────────────────────
Write-Host "`n[3/5] Installation des dépendances..." -ForegroundColor Yellow

Set-Location $ProjectDir

if (Test-Path "pnpm-lock.yaml") {
    Write-Host "  → pnpm install..." -ForegroundColor Cyan
    pnpm install --frozen-lockfile 2>&1 | Select-Object -Last 5
    Write-Host "  ✅ pnpm install terminé" -ForegroundColor Green
}

# ── Étape 4 : Build ───────────────────────────────────────────────────
if (-not $SkipBuild) {
    Write-Host "`n[4/5] Compilation..." -ForegroundColor Yellow

    Write-Host "  → cargo check (vérification du core)..." -ForegroundColor Cyan
    cargo check --manifest-path Cargo.toml 2>&1 | Select-Object -Last 5
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  ❌ cargo check a échoué. Vérifiez les erreurs ci-dessus." -ForegroundColor Red
        exit 1
    }
    Write-Host "  ✅ cargo check OK" -ForegroundColor Green

    Write-Host "  → cargo build --release (compilation du core)..." -ForegroundColor Cyan
    cargo build --manifest-path Cargo.toml --bin dadou-core --release 2>&1 | Select-Object -Last 5
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  ❌ Build échoué. Vérifiez les erreurs ci-dessus." -ForegroundColor Red
        exit 1
    }
    Write-Host "  ✅ Core compilé : target\release\dadou-core.exe" -ForegroundColor Green
}

# ── Résumé ────────────────────────────────────────────────────────────
Write-Host @"

========================================
  ✅ DADOU v1.0 — Installation terminée
========================================

Pour lancer DADOU :

  .\target\release\dadou-core.exe serve

Workspace :
  → $Workspace

Dashboard :
  → http://127.0.0.1:7790

Manuel :
  → DISTRIBUTION\manual.html

Pour changer le workspace :
  $env:OPENHUMAN_WORKSPACE = "C:\MonDossier"

Pour utiliser un provider cloud :
  → Settings > AI dans le chat, ou config.toml

========================================
"@ -ForegroundColor Green
