#!/usr/bin/env node
/**
 * Reads knip unused-exports output from a saved file and strips `export` keyword.
 * Usage: node scripts/remove-dead-exports.mjs < knip-output.txt
 *
 * knip output format (parsed):
 *   <name>  [type]  <file>:<line>:<col>
 * or with truncated paths:
 *   <name>  [type]  …<partial-file>:<line>:<col>
 */
import fs from 'fs';
import path from 'path';
import { execSync } from 'child_process';

const ROOT = process.cwd();

function resolveFile(filePart) {
  // filePart might be like "src/polyfills.ts" or "…atures/screen-intelligence/useScreenIntelligenceState.ts"
  // Remove leading dots
  let clean = filePart.replace(/^…+/, '').replace(/^\.\./, '');
  // Try common prefixes
  const candidates = [
    path.join(ROOT, clean),
    path.join(ROOT, 'app', clean),
    path.join(ROOT, clean.replace(/^[/\\]/, '')),
  ];
  for (const c of candidates) {
    try {
      if (fs.statSync(c).isFile()) return c;
    } catch {}
  }
  // Search by basename
  const basename = path.basename(clean);
  try {
    const isWin = process.platform === 'win32';
    const cmd = isWin
      ? `dir /s /b "${ROOT}\\app\\src\\${basename}" 2>nul`
      : `find "${ROOT}/app/src" -name "${basename}" 2>/dev/null`;
    const result = execSync(cmd, { encoding: 'utf8', timeout: 5000 });
    const lines = result.trim().split('\n').filter(Boolean);
    if (lines.length > 0) return lines[0].replace(/\\/g, '/');
  } catch {}
  return null;
}

let input = '';
process.stdin.on('data', chunk => (input += chunk));
process.stdin.on('end', () => {
  const lines = input.split('\n');
  let inExports = false;
  const edits = []; // { file, line }

  for (const line of lines) {
    if (line.startsWith('Unused exports')) { inExports = true; continue; }
    if (!inExports) continue;
    if (!line.trim() || line.startsWith('Unused ')) break;

    // Extract file:line:col at the end
    const m = line.match(/([\w/\\.-]+\.(?:ts|tsx|js|jsx)):(\d+):(\d+)\s*$/);
    if (!m) continue;
    const filePart = m[1];
    const lineNum = parseInt(m[2], 10);

    const resolved = resolveFile(filePart);
    if (resolved) edits.push({ file: resolved, line: lineNum });
  }

  console.log(`Found ${edits.length} export edits across ${new Set(edits.map(e => e.file)).size} files.`);

  // Group by file
  const byFile = new Map();
  for (const { file, line } of edits) {
    if (!byFile.has(file)) byFile.set(file, new Set());
    byFile.get(file).add(line);
  }

  let totalRemoved = 0;
  for (const [filePath, lineSet] of byFile) {
    const content = fs.readFileSync(filePath, 'utf-8');
    const fileLines = content.split('\n');
    const sortedLines = [...lineSet].sort((a, b) => b - a); // process bottom-up
    let fileChanged = false;

    for (const lineNum of sortedLines) {
      const idx = lineNum - 1;
      if (idx < 0 || idx >= fileLines.length) continue;
      const original = fileLines[idx];
      const trimmed = original.trim();
      let replacement = null;

      // Pattern 1: `export default Foo` → `Foo`
      if (/^export\s+default\s+/.test(trimmed)) {
        replacement = original.replace(/^(\s*)export\s+default\s+/, '$1');
      }
      // Pattern 2: `export { foo }` → `` (remove entire line)
      else if (/^export\s*\{/.test(trimmed)) {
        replacement = '';
      }
      // Pattern 3: `export function foo` / `export const foo` / `export interface Foo` → `function foo` / `const foo`
      else if (/^export\s+/.test(trimmed)) {
        replacement = original.replace(/^(\s*)export\s+/, '$1');
      }

      if (replacement !== null && replacement !== original) {
        fileLines[idx] = replacement;
        totalRemoved++;
        fileChanged = true;
      }
    }

    if (fileChanged) {
      fs.writeFileSync(filePath, fileLines.join('\n'), 'utf-8');
    }
  }

  console.log(`Removed ${totalRemoved} export declarations across ${byFile.size} files.`);
});
