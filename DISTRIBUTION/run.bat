@echo off
REM ── DADOU v1.0.0 — Lancement portable Windows ──────────────────
REM Double-cliquez ce fichier pour lancer DADOU.
REM Tout est auto-suffisant : pas d'installation, pas de dépendances.
REM
REM Prérequis inclus dans le package :
REM   - VCRUNTIME140.dll / VCRUNTIME140_1.dll (VC++ runtime)
REM
REM Prérequis système (Windows 10+ les a nativement) :
REM   - UCRT (Universal C Runtime)
REM   - Un navigateur web pour l'interface http://127.0.0.1:7788

title DADOU

:: Définir le workspace comme le dossier courant (portable)
set OPENHUMAN_WORKSPACE=%~dp0
set "APP_DIR=%~dp0"

:: Vérifier que l'exe existe
if not exist "%APP_DIR%dadou-core.exe" (
    echo [ERREUR] dadou-core.exe introuvable.
    echo L'archive semble incomplète. Ré-extrayez le zip.
    pause
    exit /b 1
)

:: Vérifier que le runtime VC++ est accessible (dans le dossier local ou le système)
:: On tente un petit test : si le DLL local est absent, on vérifie le système
if not exist "%APP_DIR%vcruntime140.dll" (
    echo [AVERTISSEMENT] vcruntime140.dll absent du dossier local.
    echo Verification de la presence systeme...
    where vcruntime140.dll >nul 2>&1
    if errorlevel 1 (
        echo [ERREUR] Visual C++ Redistributable manquant.
        echo Telechargez-le depuis : https://aka.ms/vs/17/release/vc_redist.x64.exe
        echo Ou re-extrayez l'archive DADOU complete.
        pause
        exit /b 1
    )
)

echo.
echo ╔══════════════════════════════════════════════╗
echo ║             DADOU v1.0.0                     ║
echo ║   Assistant IA autonome — mode hors-ligne     ║
echo ╚══════════════════════════════════════════════╝
echo.
echo Workspace : %OPENHUMAN_WORKSPACE%
echo Interface : http://127.0.0.1:7788
echo Dashboard : http://127.0.0.1:7790
echo.
echo Appuyez sur Ctrl+C pour quitter.
echo.

:: Lancer le core
"%APP_DIR%dadou-core.exe" serve

if errorlevel 1 (
    echo.
    echo [ERREUR] DADOU s'est arrete avec le code %errorlevel%.
    echo.
    echo Causes possibles :
    echo   - Port 7788 ou 7790 deja utilise
    echo   - Runtime VC++ manquant (installez vc_redist.x64.exe)
    echo   - Antivirus bloquant l'execution
    echo.
    echo Verifiez les logs dans : workspace\logs\
)

pause
