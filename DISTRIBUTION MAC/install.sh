#!/usr/bin/env bash
# =============================================================================
# DADOU — Script d'installation automatisée pour macOS
# =============================================================================
# Usage : bash install.sh [--offline] [--branch <name>] [--skip-tests]
#   --offline     : Mode hors-ligne (sans backend cloud)
#   --branch      : Branche git à cloner (défaut: master)
#   --skip-tests  : Ignorer la vérification des tests
# =============================================================================
set -euo pipefail

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'
STEP=0

log()  { echo -e "${GREEN}[${STEP}/5]${NC} ${BOLD}$1${NC}"; }
warn() { echo -e "${YELLOW}⚠  $1${NC}"; }

REPO_URL="https://github.com/macao100/openhuman.git"
BRANCH="master"
OFFLINE_MODE=false
SKIP_TESTS=false

for arg in "$@"; do
    case "$arg" in
        --offline) OFFLINE_MODE=true ;;
        --skip-tests) SKIP_TESTS=true ;;
        --branch) BRANCH="$2"; shift ;;
    esac
done

DADOU_DIR="${HOME}/dadou"
WORKSPACE_DIR="${HOME}/.openhuman"

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  🧠  DADOU — Installation macOS automatisée"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Répertoire : $DADOU_DIR"
echo "  Workspace  : $WORKSPACE_DIR"
echo "  Branche    : $BRANCH"
echo "  Hors-ligne : $OFFLINE_MODE"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# ── Étape 1 : Prérequis ────────────────────────────────────
STEP=1
log "Vérification des prérequis..."
bash "$(dirname "$0")/prerequis-mac.sh"
echo ""

# ── Étape 2 : Clonage ──────────────────────────────────────
STEP=2
log "Clonage du dépôt..."
if [[ -d "$DADOU_DIR" ]]; then
    warn "Le répertoire $DADOU_DIR existe déjà."
    echo "  → Mise à jour (git pull)..."
    cd "$DADOU_DIR"
    git checkout "$BRANCH"
    git pull origin "$BRANCH"
else
    git clone --branch "$BRANCH" "$REPO_URL" "$DADOU_DIR"
    cd "$DADOU_DIR"
fi

# Initialiser les submodules (CEF pour Tauri)
log "Initialisation des submodules..."
git submodule update --init --recursive 2>/dev/null || warn "Submodules non disponibles (shell Tauri uniquement)"
echo ""

# ── Étape 3 : Build ────────────────────────────────────────
STEP=3
log "Installation des dépendances Node.js (pnpm install)..."
pnpm install --frozen-lockfile

log "Compilation du core Rust (cargo build)..."
cargo build --release --bin openhuman-core
echo "  → Binaire : target/release/openhuman-core"

log "Build du frontend (pnpm build)..."
pnpm build
echo ""

# ── Étape 4 : Configuration ────────────────────────────────
STEP=4
log "Configuration de l'environnement..."

# Créer le fichier .env s'il n'existe pas
if [[ ! -f ".env" ]]; then
    cp .env.example .env
    echo "  → .env créé depuis .env.example"
fi

# Configurer le mode hors-ligne si demandé
if $OFFLINE_MODE; then
    # Remplacer/setter OPENHUMAN_OFFLINE_MODE dans .env
    if grep -q "^OPENHUMAN_OFFLINE_MODE=" .env 2>/dev/null; then
        sed -i '' 's/^OPENHUMAN_OFFLINE_MODE=.*/OPENHUMAN_OFFLINE_MODE=true/' .env
    else
        echo "OPENHUMAN_OFFLINE_MODE=true" >> .env
    fi
    echo "  → Mode hors-ligne activé (OPENHUMAN_OFFLINE_MODE=true)"
fi

# Créer app/.env.local s'il n'existe pas
if [[ ! -f "app/.env.local" ]]; then
    cp app/.env.example app/.env.local
    echo "  → app/.env.local créé depuis app/.env.example"
fi

# Créer le workspace
mkdir -p "$WORKSPACE_DIR"
echo "  → Workspace créé : $WORKSPACE_DIR"
echo ""

# ── Étape 5 : Vérification ─────────────────────────────────
STEP=5
log "Vérification de l'installation..."

# Vérifier que le binaire existe
if [[ -f "target/release/openhuman-core" ]]; then
    echo -e "  ${GREEN}✓${NC} Binaire core : target/release/openhuman-core"
else
    warn "Binaire core introuvable — la compilation a peut-être échoué"
fi

# Vérifier le frontend build
if [[ -d "app/dist" ]]; then
    echo -e "  ${GREEN}✓${NC} Frontend build   : app/dist/"
else
    warn "Frontend build introuvable — pnpm build a peut-être échoué"
fi

# Tests (optionnels)
if ! $SKIP_TESTS; then
    log "Lancement des tests Rust..."
    cargo test --lib -p dadou 2>&1 | tail -5
    echo ""
    log "Vérification ESLint..."
    (cd app && pnpm lint 2>&1) | tail -3
fi

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  🎉  Installation terminée !"
echo ""
echo "  Pour lancer DADOU :"
echo "    cd $DADOU_DIR"
echo "    pnpm dev:app          # Mode desktop (Tauri)"
echo "    cargo run -- serve     # Mode serveur (JSON-RPC)"
echo ""
echo "  Documentation :"
echo "    open DISTRIBUTION\ MAC/MANUEL-INSTALLATION.html"
echo "    open DISTRIBUTION\ MAC/MANUEL-UTILISATION.html"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
