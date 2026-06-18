#!/usr/bin/env -S pnpm exec tsx
/**
 * i18n-add-key — add a new translation key to all locale chunk files at once.
 *
 * Usage:
 *   pnpm exec tsx scripts/i18n-add-key.ts <key> <english_value> [--chunk N]
 *   pnpm i18n:add-key "settings.panels.newThing" "New Thing"
 *   pnpm i18n:add-key "nav.foo" "Foo" --chunk 1
 *
 * Without --chunk, the script guesses the chunk number from the key prefix
 * by scanning existing keys in en-{1..5}.ts.
 *
 * For non-English locales the English value is used as a placeholder so
 * CI parity checks pass immediately; translators fill in later.
 */

import { promises as fs } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const ROOT = path.resolve(path.dirname(__filename), "..");
const CHUNKS_DIR = path.join(ROOT, "app/src/lib/i18n/chunks");
const EN_FILE = path.join(ROOT, "app/src/lib/i18n/en.ts");
const CHUNK_COUNT = 5;

const LOCALES = [
  "en", "zh-CN", "hi", "es", "ar", "fr", "bn", "pt", "de", "ru",
  "id", "it", "ko", "pl",
];

interface CliArgs {
  key: string;
  value: string;
  chunk: number | null;
}

function parseArgs(): CliArgs {
  const positional: string[] = [];
  let chunk: number | null = null;

  for (let i = 2; i < process.argv.length; i++) {
    const arg = process.argv[i];
    if (arg === "--chunk" && i + 1 < process.argv.length) {
      const n = Number.parseInt(process.argv[++i], 10);
      if (n < 1 || n > CHUNK_COUNT || Number.isNaN(n)) {
        console.error(`❌ --chunk must be 1–${CHUNK_COUNT}`);
        process.exit(2);
      }
      chunk = n;
    } else if (!arg.startsWith("--")) {
      positional.push(arg);
    }
  }

  if (positional.length < 2) {
    console.error("Usage: pnpm i18n:add-key <key> <english_value> [--chunk N]");
    console.error("Example: pnpm i18n:add-key 'settings.panels.newThing' 'New Thing'");
    process.exit(2);
  }

  return { key: positional[0], value: positional[1], chunk };
}

/**
 * Guess which chunk a key belongs to by matching its prefix against
 * existing keys in each chunk. Returns the chunk with the most
 * prefix-matching keys (or 1 if no match).
 */
async function guessChunk(key: string): Promise<number> {
  const prefix = key.split(".")[0] ?? "";
  let bestChunk = 1;
  let bestScore = 0;

  for (let n = 1; n <= CHUNK_COUNT; n++) {
    const chunkPath = path.join(CHUNKS_DIR, `en-${n}.ts`);
    const content = await fs.readFile(chunkPath, "utf-8");
    // Count how many lines start with a key sharing the same first segment
    const re = new RegExp(`^\\s+'${prefix}\\.`, "gm");
    const matches = content.match(re);
    const score = matches ? matches.length : 0;
    if (score > bestScore) {
      bestScore = score;
      bestChunk = n;
    }
  }

  return bestChunk;
}

/**
 * Insert `line` into the last position of the TranslationMap object
 * literal in `content` (right before the closing `};`).
 */
function insertIntoMap(content: string, line: string): string {
  const closing = content.lastIndexOf("};");
  if (closing === -1) {
    throw new Error("Could not find closing `};` in chunk file");
  }
  // Find the line before `};` — insert new key with proper indentation
  const beforeClose = content.slice(0, closing);
  const afterClose = content.slice(closing);
  // Ensure trailing comma on previous line if needed
  const trimmed = beforeClose.trimEnd();
  return `${trimmed}\n  ${line},\n${afterClose}`;
}

async function keyExistsInEn(key: string): Promise<boolean> {
  const content = await fs.readFile(EN_FILE, "utf-8");
  return content.includes(`'${key}':`);
}

async function main() {
  const { key, value, chunk: explicitChunk } = parseArgs();

  if (await keyExistsInEn(key)) {
    console.error(`❌ Key '${key}' already exists in en.ts`);
    process.exit(1);
  }

  const chunk = explicitChunk ?? (await guessChunk(key));
  const entryLine = `'${key}': '${value.replace(/'/g, "\\'")}'`;

  console.log(`🔑 Adding key:  '${key}'`);
  console.log(`📝 Value:       '${value}'`);
  console.log(`📦 Chunk:       ${chunk} (${explicitChunk ? "explicit" : "guessed"})`);
  console.log("");

  // 1. Add to en.ts (source of truth)
  let enContent = await fs.readFile(EN_FILE, "utf-8");
  enContent = insertIntoMap(enContent, entryLine);
  await fs.writeFile(EN_FILE, enContent, "utf-8");
  console.log("✅ en.ts");

  // 2. Add to all locale chunk files
  for (const locale of LOCALES) {
    const chunkPath = path.join(CHUNKS_DIR, `${locale}-${chunk}.ts`);
    let chunkContent: string;
    try {
      chunkContent = await fs.readFile(chunkPath, "utf-8");
    } catch {
      console.error(`❌ Missing chunk file: ${locale}-${chunk}.ts`);
      process.exit(1);
    }
    // Non-English locales get the English value as placeholder
    const localeEntryLine =
      locale === "en"
        ? entryLine
        : `'${key}': '${value.replace(/'/g, "\\'")}' /* TODO: translate */`;
    chunkContent = insertIntoMap(chunkContent, localeEntryLine);
    await fs.writeFile(chunkPath, chunkContent, "utf-8");
  }
  console.log(`✅ All ${LOCALES.length} locale chunk files updated`);

  // 3. Verify with i18n:check
  console.log("\n🔍 Running i18n:check to verify...");
  const { execSync } = await import("node:child_process");
  try {
    execSync("pnpm i18n:check", { cwd: ROOT, stdio: "inherit" });
    console.log("\n✅ i18n:check passed — all locales in sync.");
  } catch {
    console.error("\n❌ i18n:check failed. Review the changes manually.");
    process.exit(1);
  }
}

main().catch((err) => {
  console.error("❌", err instanceof Error ? err.message : String(err));
  process.exit(1);
});
