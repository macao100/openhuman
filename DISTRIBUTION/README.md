# DADOU v1.0 — Distribution

**Assistant IA personnel open-source (GPL-3.0)**

---

## Contenu du dossier DISTRIBUTION

| Fichier | Description |
|---------|-------------|
| `install.html` | Assistant d'installation interactif — vérifie les prérequis, configure l'environnement, compile et lance DADOU |
| `manual.html` | Manuel d'utilisation complet avec 5 cas d'usage réels (développeur, data scientist, maker, juriste, étudiant) |
| `setup.ps1` | Script PowerShell automatisé — installe tous les prérequis (winget), configure LIBCLANG_PATH, lance le build |
| `README.md` | Ce fichier |

---

## Prérequis

| Outil | Version min | Vérification |
|-------|------------|-------------|
| Windows 11 / Linux / macOS | — | Compatible |
| Rust | 1.93 | `rustc --version` |
| Node.js | 24 | `node --version` |
| pnpm | 10 | `pnpm --version` |
| CMake | 3.x | `cmake --version` |
| LLVM + libclang | 18+ | `clang --version` |
| Git | 2.x | `git --version` |
| Docker (optionnel) | 20+ | `docker --version` |

---

## Installation rapide (PowerShell)

```powershell
# 1. Ouvrir PowerShell en administrateur
# 2. Exécuter le script d'installation
.\DISTRIBUTION\setup.ps1

# 3. Alternative : ouvrir install.html dans un navigateur
#    et suivre l'assistant interactif
```

## Démarrage

```powershell
# Tout-en-un : le core sert l'API + le chat + le dashboard
.\target\release\dadou-core.exe serve

# Chat (interface React)
# → http://127.0.0.1:7788

# Dashboard (observabilité temps réel)
# → http://127.0.0.1:7790

# Manuel
# → DISTRIBUTION\manual.html
```

**Un seul binaire, une seule commande.** Le frontend React est servi directement par le core depuis `app/dist/`.

---

## Structure du projet

```
dadou/
├── DISTRIBUTION/          # Fichiers de déploiement
│   ├── install.html       # Assistant d'installation
│   ├── manual.html        # Manuel utilisateur
│   ├── setup.ps1          # Script d'installation automatisée
│   ├── run.bat / run.ps1  # Lancement portable (double-clic)
│   ├── config.toml        # Configuration portable
│   └── README.md          # Ce fichier
├── src/                   # Core Rust (60+ domaines)
├── app/                   # Frontend React + Tauri
├── target/release/        # Binaires compilés
├── .planning/             # Roadmap et spécifications
└── Cargo.toml             # Manifest Rust
```

## Architecture runtime

**Un seul binaire, zéro dépendance externe.** Le core Rust (`dadou-core.exe`) sert :
- L'API JSON-RPC sur `http://127.0.0.1:7788/rpc`
- L'interface React (SPA) sur `http://127.0.0.1:7788`
- Le dashboard d'observabilité sur `http://127.0.0.1:7790`

Le frontend est servi directement depuis `app/dist/` par le core — pas besoin de serveur web séparé.

## Mode offline

Par défaut, DADOU démarre en mode 100% local (`offline_mode = true` dans `config.toml`).
- Aucune connexion à api.tinyhumans.ai
- Session locale auto-signée (token JWT-like, expiry 1 an)
- Fonctionne avec Ollama, DeepSeek, OpenAI, ou tout provider compatible

---

## Licence

DADOU est distribué sous licence GPL-3.0. Tout fork doit rester sous GPL-3.0.

OpenHuman original : Copyright (c) Tiny Humans AI Inc. — https://github.com/tinyhumansai/openhuman
