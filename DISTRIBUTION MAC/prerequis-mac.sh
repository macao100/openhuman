#!/usr/bin/env bash
# =============================================================================
# DADOU — Vérification et installation des prérequis macOS
# =============================================================================
# Usage : bash prerequis-mac.sh [--install]
#   Sans --install : vérifie uniquement (dry-run)
#   Avec --install : installe les prérequis manquants
# =============================================================================
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'

INSTALL_MODE=false
[[ "${1:-}" == "--install" ]] && INSTALL_MODE=true

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  DADOU — Vérification des prérequis macOS"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# ── macOS version ──────────────────────────────────────────
OS_VERS=$(sw_vers -productVersion 2>/dev/null || echo "0")
OS_MAJOR=$(echo "$OS_VERS" | cut -d. -f1)
echo -n "macOS version : $OS_VERS "
if [[ "$OS_MAJOR" -ge 14 ]]; then
    echo -e "${GREEN}✓${NC}"
else
    echo -e "${YELLOW}⚠ (Sonoma 14+ recommandé)${NC}"
fi

# ── Xcode Command Line Tools ─────────────────────────────────
echo -n "Xcode CLI Tools : "
if xcode-select -p &>/dev/null; then
    echo -e "${GREEN}✓ ($(xcode-select -p))${NC}"
else
    echo -e "${RED}✗ absent${NC}"
    if $INSTALL_MODE; then
        echo "  → Installation en cours..."
        xcode-select --install 2>/dev/null || true
        echo "  → Suivez la boîte de dialogue. Relancez ce script après installation."
    fi
fi

# ── Homebrew ────────────────────────────────────────────────
echo -n "Homebrew : "
if command -v brew &>/dev/null; then
    BREW_VER=$(brew --version | head -1)
    echo -e "${GREEN}✓ ($BREW_VER)${NC}"
else
    echo -e "${RED}✗ absent${NC}"
    if $INSTALL_MODE; then
        echo "  → Installation de Homebrew..."
        /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
    fi
fi

# ── Rust ────────────────────────────────────────────────────
echo -n "Rust (rustup + cargo) : "
if command -v rustup &>/dev/null && command -v cargo &>/dev/null; then
    RUST_VER=$(rustc --version | awk '{print $2}')
    echo -e "${GREEN}✓ (rustc $RUST_VER)${NC}"
else
    echo -e "${RED}✗ absent${NC}"
    if $INSTALL_MODE; then
        echo "  → Installation de Rust via rustup..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source "$HOME/.cargo/env"
    fi
fi

# ── Node.js ─────────────────────────────────────────────────
echo -n "Node.js (≥24) : "
if command -v node &>/dev/null; then
    NODE_VER=$(node --version | sed 's/v//')
    NODE_MAJOR=$(echo "$NODE_VER" | cut -d. -f1)
    if [[ "$NODE_MAJOR" -ge 24 ]]; then
        echo -e "${GREEN}✓ (v$NODE_VER)${NC}"
    else
        echo -e "${YELLOW}⚠ v$NODE_VER (v24+ recommandé)${NC}"
    fi
else
    echo -e "${RED}✗ absent${NC}"
    if $INSTALL_MODE; then
        echo "  → Installation de Node.js 24 via Homebrew..."
        brew install node@24
    fi
fi

# ── pnpm ────────────────────────────────────────────────────
echo -n "pnpm (≥10.10) : "
if command -v pnpm &>/dev/null; then
    PNPM_VER=$(pnpm --version)
    echo -e "${GREEN}✓ (v$PNPM_VER)${NC}"
else
    echo -e "${RED}✗ absent${NC}"
    if $INSTALL_MODE; then
        echo "  → Installation de pnpm..."
        npm install -g pnpm@10.10.0
    fi
fi

# ── Git ─────────────────────────────────────────────────────
echo -n "Git : "
if command -v git &>/dev/null; then
    GIT_VER=$(git --version | awk '{print $3}')
    echo -e "${GREEN}✓ (v$GIT_VER)${NC}"
else
    echo -e "${RED}✗ absent${NC}"
    if $INSTALL_MODE; then
        echo "  → Installation de Git..."
        brew install git
    fi
fi

# ── Ollama (optionnel) ──────────────────────────────────────
echo -n "Ollama (optionnel, IA locale) : "
if command -v ollama &>/dev/null; then
    OLLAMA_VER=$(ollama --version 2>/dev/null | head -1 || echo "installé")
    echo -e "${GREEN}✓ ($OLLAMA_VER)${NC}"
else
    echo -e "${YELLOW}⚠ absent (téléchargement IA local indisponible)${NC}"
    if $INSTALL_MODE; then
        echo "  → Installation d'Ollama..."
        curl -fsSL https://ollama.com/install.sh | sh
    fi
fi

# ── Docker (optionnel, pour skills Python) ──────────────────
echo -n "Docker (optionnel, skills Python) : "
if command -v docker &>/dev/null; then
    DOCKER_VER=$(docker --version | awk '{print $3}' | sed 's/,//')
    echo -e "${GREEN}✓ (v$DOCKER_VER)${NC}"
else
    echo -e "${YELLOW}⚠ absent${NC}"
fi

# ── Espace disque ───────────────────────────────────────────
echo -n "Espace disque : "
AVAIL_GB=$(df -g . | tail -1 | awk '{print $4}')
echo -e "${GREEN}${AVAIL_GB} Go disponibles${NC}"
if [[ "$AVAIL_GB" -lt 10 ]]; then
    echo -e "${YELLOW}⚠ Moins de 10 Go disponibles. La build peut échouer.${NC}"
fi

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
if $INSTALL_MODE; then
    echo "  ✅ Prérequis vérifiés et installés."
else
    echo "  ✅ Vérification terminée."
    echo "  Pour installer les prérequis manquants :"
    echo "    bash prerequis-mac.sh --install"
fi
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
