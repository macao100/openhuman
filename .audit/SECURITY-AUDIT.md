# RAPPORT D'AUDIT DE SÉCURITÉ — DADOU (OpenHuman)

**Date :** 2026-06-16
**Périmètre :** Core Rust (`src/`), Frontend TypeScript/React (`app/src/`), Tauri shell (`app/src-tauri/`)
**Type :** Audit statique read-only (Stage 2)
**Score global : 7.5 / 10**

| Catégorie | Statut | Score |
|---|---|---|
| Secrets hardcodés | ✅ Aucun trouvé | 10/10 |
| Dépendances critiques | ⚠️ À surveiller | 7/10 |
| Permissions Tauri | ⚠️ CSP excessive | 5/10 |
| Pratiques crypto | ✅ Solides | 9/10 |
| Input validation (unwrap) | ⚠️ Risques modérés | 6/10 |
| Error handling | ✅ Bonnes pratiques | 8/10 |
| **Moyenne pondérée** | | **7.5/10** |

---

## 1. SECRETS HARDCODÉS — OK (10/10)

**Aucun secret hardcodé trouvé** dans le code source. Le projet dispose de mécanismes de détection et de scrutation complets :

### Rust — Scrutation centralisée (`src/main.rs:200-242`)
- `SECRET_PATTERNS` : 7 patterns regex couvrant Bearer tokens, api_key, tokens génériques, clés Anthropic (`sk-ant-*`), clés OpenAI admin (`sk-admin-*`), clés OpenAI projet (`sk-proj-*`/`sk-org-*`), et catch-all `sk-*`
- Fonction `scrub_secrets()` appliquée à toutes les sorties de logs et rapports d'erreur Sentry

### Rust — RPC log redaction (`src/core/rpc_log.rs:71-84`)
- `is_sensitive_key()` filtre : `api_key`, `apikey`, `token`, `access_token`, `refresh_token`, `authorization`, `password`, `secret`, `client_secret`
- `redact_params_for_log()` et `redact_result_for_trace()` appliquent la redaction récursivement

### Frontend — Sanitization (`app/src/utils/sanitize.ts`)
- 24 clés sensibles détectées et remplacées par `[REDACTED]` dans les logs
- Détection des patterns via regex (password, secret, token, key, auth, credential)
- `sanitizeError()` : stack trace exposée uniquement en dev (`IS_DEV`)

### Frontend — Prompt injection guard (`app/src/chat/promptInjectionGuard.ts`)
- Détection de fuite de secrets via regex : `api\s*key|secret|token|password|private\s+key|credentials?|session\s+cookie|jwt|bearer`
- 4 règles de détection : override instructions, role hijack, exfiltration de system prompt, exfiltration de secrets
- Seuils : score >= 0.7 → `block`, >= 0.45 → `review`

**Verdict :** Aucun secret hardcodé. La scrutation est systématique et bien architecturée.

---

## 2. DÉPENDANCES CRITIQUES — ⚠️ À SURVEILLER (7/10)

### Rust (Cargo.toml) — Principales dépendances

| Dépendance | Version | Notes |
|---|---|---|
| `aes-gcm` | 0.10 | Correcte, AEAD |
| `argon2` | 0.5 | Correcte, Argon2id |
| `reqwest` | 0.12 | Correcte, TLS multiple |
| `tokio` | 1 (full) | Correcte |
| `ring` | 0.17 | ⚠️ ring 0.17 a eu des advisories passés ; 0.18 est sorti (2025) |
| `rustls` | 0.23 | Correcte |
| `wasmtime` | 29 | Correcte pour sandbox WASM |
| `sentry` | 0.47.0 | Correcte |
| `rusqlite` | 0.37 | Correcte (bundled SQLite) |

### Frontend (package.json) — Principales dépendances

| Dépendance | Version | Notes |
|---|---|---|
| `react` / `react-dom` | ^19.1.0 | Correcte |
| `@tauri-apps/api` | ^2.10.0 | Correcte |
| `socket.io-client` | ^4.8.3 | Correcte |
| `@sentry/react` | ^10.38.0 | Correcte |

### Risques identifiés

1. **ring 0.17** — 0.18 est disponible avec des correctifs de sécurité. Mise à jour recommandée.
2. **wasmtime 29** — Version majeure correcte, mais suivre les CVE régulièrement (moteur WASM critique).
3. **Aucun `cargo audit` ou `cargo deny` configuré** dans la CI — absence de scan CVE automatisé.

**Verdict :** Versions globalement à jour. ring 0.18 serait préférable. CI pourrait intégrer `cargo audit`.

---

## 3. PERMISSIONS TAURI — ⚠️ CSP EXCESSIVE (5/10)

### Capacités (`capabilities/default.json`)

Permissions déclarées : `core:default`, `core:window:default`, fenêtrage (hide/show/set-focus/unminimize/dragging/always-on-top), `deep-link:default`, `notification:default`, `opener:default`, `updater:default`, permissions custom (`allow-core-process`, `allow-workspace-files`, `allow-app-update`, `allow-loopback-oauth`).

**Évaluation :** Les permissions sont cohérentes avec une application desktop. Aucune permission excessive évidente.

### Content Security Policy (`tauri.conf.json`)

```csp
default-src 'self' 'unsafe-inline' data: blob: https: wss: ipc: http://ipc.localhost http://127.0.0.1:* http://localhost:*;
script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval';
connect-src 'self' ipc: http://ipc.localhost http://127.0.0.1:* http://localhost:* http: ws://127.0.0.1:* ws://localhost:* ws: https: wss: data: blob:;
img-src 'self' data: blob: https:;
frame-src 'self' https: data: blob:;
```

#### Problèmes identifiés

| Problème | Sévérité | Détail |
|---|---|---|
| `script-src 'unsafe-inline'` | 🔴 CRITICAL | Permet l'exécution de scripts inline — annule la protection XSS primaire du CSP |
| `script-src 'wasm-unsafe-eval'` | ⚠️ HIGH | Permet l'exécution de WebAssembly sans restrictions |
| `connect-src http: https: ws: wss:` | ⚠️ HIGH | Permet des connexions réseau vers n'importe quelle origine |
| `frame-src https:` | ⚠️ MEDIUM | Permet d'intégrer n'importe quelle page HTTPS dans une iframe |
| `default-src 'unsafe-inline'` | ⚠️ MEDIUM | Dilue l'effet du default-src |

**Verdict :** CSP trop permissive. L'utilisation de `'unsafe-inline'` dans `script-src` est compréhensible pour une app React/Vite en développement, mais une version de production devrait utiliser un nonce ou un hash. Recommandation forte : resserrer la CSP pour la production.

---

## 4. PRATIQUES CRYPTO — ✅ SOLIDES (9/10)

### Chiffrement local (`src/openhuman/encryption/core.rs`)
- **Algorithme :** AES-256-GCM (AEAD, mode authentifié)
- **Dérivation de clé :** Argon2id (Algorithm::Argon2id, Version::V0x13)
- **Paramètres Argon2 :** 65536 itérations mémoire, 3 parallélisme, sortie 32 octets
- **Nonce :** Aléatoire (OsRng), 96 bits, nouveau à chaque chiffrement
- **Erreurs :** Toutes les erreurs sont propagées via `Result<_, String>` — pas d'unwrap, pas de fuite de clé

### Stockage de secrets — Keyring (`src/openhuman/keyring/crypto.rs`)
- **Algorithme :** ChaCha20-Poly1305 (AEAD)
- **Nonce :** Aléatoire (OsRng), 96 bits
- **Format :** nonce || ciphertext || tag — validation de longueur au déchiffrement
- **Erreurs :** Messages d'erreur génériques ("decryption failed — wrong key or tampered data")

### Tunnel device (`src/openhuman/devices/crypto.rs`)
- **Key agreement :** X25519 (x25519-dalek `static_secrets`)
- **Chiffrement :** XChaCha20-Poly1305 (nonce 192 bits)
- **Protection anti-rejeu :** Fenêtre glissante de 128 entrées
- **Dérivation :** DH statique-éphémère, clé partagée 32 octets

### API Backend (`src/api/rest.rs:980-988`)
- AES-256-GCM avec clé dérivée et nonce aléatoire (IV passé en paramètre)

### Points d'amélioration

| Finding | Sévérité | Fichier | Détail |
|---|---|---|---|
| `EncryptedPayload.salt` vide lors de l'encryption | ⚠️ LOW | `encryption/core.rs:72` | Le salt est stocké séparément dans le fichier de clé, ce qui est correct mais le champ `salt` dans le payload est toujours `Vec::new()` — documentation insuffisante |
| SHA-1 utilisé | ⚠️ LOW | `Cargo.toml:65` | SHA-1 documenté comme legacy et utilisé UNIQUEMENT pour Tencent COS HMAC-SHA1 (non security-sensitive) |
| Clé maîtresse en mémoire | ⚠️ MEDIUM | `keyring/ops.rs` | Aucun zeroing explicite de la clé après usage (mémoire potentiellement dumpable) |

**Verdict :** Chiffrement solide. Modes AEAD (GCM, ChaCha20-Poly1305), nonces aléatoires, key derivation via Argon2id/X25519. PAS d'ECB, PAS de clés statiques, PAS d'IV fixes. Les quelques points faibles sont mineurs.

---

## 5. INPUT VALIDATION (UNWRAP) — ⚠️ RISQUES MODÉRÉS (6/10)

### unwrap() en production (hors tests)

| Fichier | Ligne | Pattern | Sévérité | Justification |
|---|---|---|---|---|
| `src/main.rs` | 203, 206, 211, 217, 222, 227, 231 | `Regex::new(...).unwrap()` | 🔴 CRITIQUE | Regex statiques en `Lazy` — panique au démarrage si une regex est invalide. Justifié car patterns validés manuellement, mais pas de `?` ni `expect()` avec message. |
| `src/openhuman/security/bubblewrap.rs` | 105, 138, 163 | `sandbox.wrap_command(&mut cmd).unwrap()` | 🔴 CRITIQUE | Panique si bubblewrap n'est pas installé ou configuré. Un appel système échoué → crash complet du processus. |
| `src/openhuman/security/firejail.rs` | 136, 175 | `sandbox.wrap_command(&mut cmd).unwrap()` | 🔴 CRITIQUE | Même problème que bubblewrap. Panique si firejail est absent. |
| `src/openhuman/devices/crypto.rs` | 60 | `peer_arr: [u8; 32] = peer_bytes.try_into().unwrap()` | ⚠️ MEDIUM | Précédé d'un check de longueur (lignes 54-58), l'unwrap est effectivement safe. Préférer `.expect("peer pubkey validated")` |
| `src/openhuman/devices/crypto.rs` | 148 | `nonce_bytes: [u8; NONCE_LEN] = frame[1..1+NONCE_LEN].try_into().unwrap()` | ⚠️ MEDIUM | Précédé d'un check de taille (lignes 144-146), safe. |
| `src/rpc/mod.rs` | 98, 106, 110, 127, 134, 144, 152 | `into_cli_compatible_json().unwrap()` | ✅ Tests uniquement | Dans des blocs `#[cfg(test)]` ou `#[test]` — acceptable. |

### unwrap() à corriger en priorité

1. **`src/openhuman/security/bubblewrap.rs:105, 138, 163`** — Remplacer `unwrap()` par `?` ou `expect()` avec message contextuel. Panique si bubblewrap absent → DoS.
2. **`src/openhuman/security/firejail.rs:136, 175`** — Idem. Panique si firejail absent.
3. **`src/main.rs:203-231`** — Ajouter des messages `expect()` descriptifs pour les regex en `Lazy`.

---

## 6. ERROR HANDLING — ✅ BONNES PRATIQUES (8/10)

### Points forts
- **Typage strict :** `thiserror` pour erreurs de bibliothèque, `anyhow` pour erreurs applicatives
- **RPC errors :** `Result<Value, String>` avec messages sanitizés — pas de fuite de contexte interne
- **StructuredRpcError** (`src/rpc/structured_error.rs:36`) : sentinel-prefixed, `expected_user_state: true` évite Sentry pour les états utilisateur normaux
- **Redaction logs RPC** (`src/core/rpc_log.rs`) : sensible key filtering avant écriture
- **Secret scrubbing Sentry** (`src/main.rs:236-242`) : `scrub_secrets()` appliqué avant envoi à Sentry
- **Frontend sanitize.ts** : `sanitizeError()` expose stack trace seulement en dev, messages génériques en prod
- **Validation d'URL workspace** (`app/src/utils/workspaceLinks.test.ts`) : chemins avec `..`, `%2e%2e`, null bytes rejetés
- **Validation d'URL Ollama** (`app/src/utils/ollamaUrlValidation.ts:35`) : username/password dans l'URL détectés

### Points faibles

| Finding | Sévérité | Fichier | Détail |
|---|---|---|---|
| Messages d'erreur crypto potentiellement verbaux | ⚠️ LOW | `encryption/core.rs:38,59,67` | Incluent le message d'erreur interne (`Argon2 params error: {e}`) — mais pas de données sensibles |
| Pas de limite de taille sur les logs d'erreur | ⚠️ MEDIUM | Plusieurs fichiers | Une erreur contenant un long payload pourrait saturer les logs (potentiel OOM disk) |
| Frontend `sanitizeError` expose `stack` en dev | ⚠️ LOW | `sanitize.ts:79` | Protégé par `IS_DEV` mais pourrait fuire en cas d'erreur de configuration |

---

## 7. UNSAFE BLOCKS

| Fichier | Lignes | Usage | Risque |
|---|---|---|---|
| `src/core/jsonrpc.rs` | 1559 | FFI / manipulation bas niveau | ⚠️ MEDIUM — nécessite revue |
| `src/openhuman/workspace/ops.rs` | 132, 143 | Opérations fichiers | ⚠️ MEDIUM |
| `src/openhuman/cwd_jail/jail.rs` | 129, 142, 155, 169 | Gestion de processus Windows (raw handle) | ⚠️ MEDIUM — attendu pour du process management |
| `src/openhuman/cwd_jail/windows.rs` | 90, 356, 366, 376 | Process sandboxing Windows | ⚠️ MEDIUM |
| `src/openhuman/cwd_jail/windows_restricted.rs` | 281, 606, 618 | Process sandboxing Windows restreint | ⚠️ MEDIUM |
| `src/openhuman/people/address_book.rs` | 130, 173 | FFI carnet d'adresses | ⚠️ LOW |
| `src/openhuman/security/bubblewrap.rs` | in `sandbox` test | Tests | ✅ |
| Tests | multiples | Divers | ✅ Tests uniquement |

**Tous les unsafe blocks sont localisés dans du code système attendu** (gestion de processus, sandboxing, FFI). Aucun dans la logique métier. Cependant, **aucune commentaire `// SAFETY:` n'a été vérifié** — une revue manuelle est recommandée.

---

## 8. PROMPT INJECTION — DÉFENSE EN PROFONDEUR

- ✅ Guard frontend (`promptInjectionGuard.ts`) : 4 règles de détection, scoring, seuils block/review/allow
- ✅ Regex de détection de fuite de secrets dans les prompts
- ✅ Détection de leetspeak (remplacement 0→o, 1→i, 3→e, etc.)
- ✅ Détection de caractères Unicode zero-width
- ✅ Détection de Base64-like content (score +0.08)

---

## TABLEAU DE BORD GLOBAL

| Catégorie | 🔴 CRITICAL | ⚠️ HIGH | 🔶 MEDIUM | 💡 LOW |
|---|---|---|---|---|
| Secrets hardcodés | 0 | 0 | 0 | 0 |
| Dépendances | 0 | 0 | 0 | 2 |
| Permissions Tauri | 1 | 2 | 1 | 0 |
| Crypto | 0 | 0 | 1 | 2 |
| unwrap() production | 3 | 0 | 2 | 0 |
| Error handling | 0 | 0 | 1 | 2 |
| unsafe blocks | 0 | 0 | 5 | 1 |
| **Total** | **4** | **2** | **10** | **7** |

---

## RECOMMANDATIONS PRIORITAIRES

### 🔴 CRITICAL — Corriger immédiatement
1. **CSP `'unsafe-inline'` dans script-src** : Utiliser un nonce CSP ou des hash pour les scripts inline en production
2. **`bubblewrap.rs` unwrap()** : Remplacer par propagation d'erreur (`?`) — panique si l'outil de sandboxing est absent
3. **`firejail.rs` unwrap()** : Même correctif que bubblewrap
4. **`main.rs` Regex unwrap()** : Ajouter des messages `expect()` descriptifs pour diagnostiquer rapidement les regex invalides

### ⚠️ HIGH — Corriger rapidement
1. **CSP `connect-src` trop large** : Restreindre aux domaines spécifiques que l'application contacte
2. **CSP `wasm-unsafe-eval`** : Évaluer si WASM est nécessaire et documenter
3. **ring 0.17 → 0.18** : Mise à jour recommandée

### 🔶 MEDIUM — Planifier
1. **Zeroing mémoire clé** : Ajouter `zeroize` ou équivalent pour les clés en mémoire
2. **Audit CI/CD `cargo audit`** : Ajouter un scan CVE automatique
3. **Limite de taille logs d'erreur** : Empêcher saturation disque
4. **Revue des unsafe blocks** : Vérifier la présence des commentaires `// SAFETY:`
5. **Documentation `EncryptedPayload.salt`** : Clarifier pourquoi le champ est toujours vide

### 💡 LOW — Amélioration continue
1. Tests de fuite de mémoire crypto (mlock/mprotect pour clés)
2. Revue des messages d'erreur pour éliminer toute information sensible résiduelle
3. Envisager `secretszero` ou `zeroize` pour les mots de passe en mémoire

---

*Rapport généré par audit statique. Certains findings peuvent nécessiter une vérification manuelle pour confirmation.*
