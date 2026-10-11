#!/usr/bin/env node
/**
 * Secret guard.
 *
 * Fails if anything that looks like a credential is committed, or if a
 * placeholder that is known to be *meant* to be empty is filled in. This runs
 * in CI on every platform, and locally before a release build.
 *
 * It never prints a matched value — only the file, the line and which pattern
 * matched — because a guard that echoes a leaked key to a build log leaks it a
 * second time.
 *
 * Usage:  node scripts/check-secrets.mjs
 * Exit:   0 clean, 1 findings
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(fileURLToPath(new URL(".", import.meta.url)), "..");

const SKIP_DIRS = new Set(["node_modules", "target", "dist", ".git", ".freebuff"]);

const TEXT_FILES = /\.(rs|ts|tsx|js|mjs|cjs|json|toml|md|html|css|txt|yml|yaml|sh|ps1|plist|entitlements|env|example|local)$/;

/**
 * Files that legitimately discuss credentials. They contain documentation of
 * where secrets belong, never a secret: verified by the patterns below, which
 * require a value of realistic length rather than a bare mention.
 */
const DOCUMENTATION = new Set([
  ".env.example",
  "README.md",
  "BRANDING_AUDIT.md",
  "RELEASE_READINESS.md",
  "TEST_REPORT.md",
  "docs/SECURITY.md",
  "docs/BUILD_MACOS.md",
  "docs/BUILD_WINDOWS.md",
  "docs/TROUBLESHOOTING.md",
  "docs/PRIVACY.md",
  "src-tauri/core/src/identity.rs",
  "scripts/check-secrets.mjs",
]);

const PATTERNS = [
  { name: "OpenAI-style key", re: /\bsk-[A-Za-z0-9_-]{20,}/ },
  { name: "AWS access key id", re: /\bAKIA[0-9A-Z]{16}\b/ },
  { name: "private key block", re: /-----BEGIN [A-Z ]*PRIVATE KEY-----/ },
  { name: "GitHub token", re: /\bgh[pousr]_[A-Za-z0-9]{20,}/ },
  { name: "Google API key", re: /\bAIza[0-9A-Za-z_-]{35}\b/ },
  { name: "Slack token", re: /\bxox[baprs]-[A-Za-z0-9-]{10,}/ },
  // A populated assignment, not a mention: requires a quoted value of >= 16
  // characters, so `API_KEY=""` and prose about API keys never match.
  {
    name: "populated credential assignment",
    re: /(api[_-]?key|apikey|password|passwd|secret|token|private[_-]?key)\s*[:=]\s*["'`][^"'`\s]{16,}["'`]/i,
  },
];

function* walk(dir) {
  for (const entry of readdirSync(dir)) {
    if (SKIP_DIRS.has(entry)) continue;
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) yield* walk(path);
    else if (TEXT_FILES.test(entry) || /\.(example|env)$/.test(entry)) yield path;
  }
}

const findings = [];

for (const path of walk(ROOT)) {
  const rel = relative(ROOT, path).split("\\").join("/");
  const lines = readFileSync(path, "utf8").split(/\r?\n/);

  for (const [index, line] of lines.entries()) {
    for (const { name, re } of PATTERNS) {
      if (!re.test(line)) continue;
      // Documentation may name a pattern, but never hold a value: require that
      // the match is not inside a fenced example or a placeholder.
      const placeholder =
        /(your|example|placeholder|redacted|xxx+|<|\.\.\.|CHANGE_ME|PLACEHOLDER|app-specific|TEAMID|you@)/i.test(
          line
        );
      if (DOCUMENTATION.has(rel) && placeholder) continue;
      findings.push(`${rel}:${index + 1}: ${name}`);
    }
  }
}

if (findings.length > 0) {
  console.error("secret guard: FAIL");
  for (const finding of findings) console.error(`  ${finding}`);
  console.error(`\n${findings.length} finding(s). Values are deliberately not printed.`);
  process.exit(1);
}

console.log("secret guard: PASS (no credentials or populated secrets found)");
