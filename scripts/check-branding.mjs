#!/usr/bin/env node
/**
 * Branding guard.
 *
 * Fails the build if a retired product name reappears anywhere in the shipping
 * tree, or if a brand-collision name creeps in. It exists because the branding
 * audit in BRANDING_AUDIT.md is only worth the enforcement behind it: without a
 * machine check, the next person to paste a snippet from another project
 * silently re-introduces the old identity.
 *
 * Usage:  node scripts/check-branding.mjs
 * Exit:   0 clean, 1 findings
 *
 * The allowlist below is exhaustive and each entry is justified. Anything else
 * that matches is a finding.
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(fileURLToPath(new URL(".", import.meta.url)), "..");

/** Directories that are generated, vendored or not part of the product. */
const SKIP_DIRS = new Set(["node_modules", "target", "dist", ".git", ".freebuff"]);

/** Extensions worth scanning. Everything else is treated as binary. */
const TEXT_FILES = /\.(rs|ts|tsx|js|mjs|cjs|json|toml|md|html|css|txt|yml|yaml|sh|ps1|plist|entitlements|gitignore|example)$/;

/**
 * Files allowed to contain a retired name, with the reason.
 *
 * Every one of these is a *migration* or *reporting* concern. Nothing here is
 * executed to brand the product, and nothing here writes a retired name.
 */
const ALLOWED = new Map([
  ["BRANDING_AUDIT.md", "the audit report: it records what was removed"],
  ["RELEASE_READINESS.md", "the release report: it cites the migration identifiers"],
  [".env.example", "documents that the previous environment-variable names are honoured as a fallback"],
  ["src/identity.ts", "LEGACY_STORAGE_PREFIX, read once so an upgrade keeps the user's theme"],
  ["src-tauri/core/src/identity.rs", "the LEGACY_* migration constants"],
  ["src-tauri/core/src/migration.rs", "migration implementation and its test fixtures"],
  ["scripts/check-branding.mjs", "defines the patterns this guard searches for"],
]);

/** Patterns that must never appear. `retired` may appear in allowlisted files. */
const RETIRED = [
  { name: "previous product name", re: /fluid\s?voice/i },
  { name: "previous internal name", re: /jarvis/i },
];

/**
 * Files allowed to *name* a brand-collision candidate: the documents that
 * record the decision, and the guard that enforces it. Product code, UI copy
 * and configuration are never allowed to.
 */
const COLLISION_ALLOWED = new Set([
  "BRANDING_AUDIT.md",
  "RELEASE_READINESS.md",
  "scripts/check-branding.mjs",
]);

/** Patterns that must not appear outside those documents. */
const FORBIDDEN_EVERYWHERE = [
  { name: "brand collision: FluidPrompt", re: /fluidprompt/i },
  { name: "brand collision: AirVoice", re: /airvoice/i },
  { name: "brand collision: FluidAir", re: /fluidair/i },
  { name: "brand collision: Fluid Assistant", re: /fluid\s?assistant/i },
  { name: "brand collision: AirPrompt Pro", re: /airprompt[\s_-]?pro/i },
];

function* walk(dir) {
  for (const entry of readdirSync(dir)) {
    if (SKIP_DIRS.has(entry)) continue;
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) yield* walk(path);
    else if (TEXT_FILES.test(entry)) yield path;
  }
}

const findings = [];

for (const path of walk(ROOT)) {
  const rel = relative(ROOT, path).split("\\").join("/");
  const lines = readFileSync(path, "utf8").split(/\r?\n/);

  for (const [index, line] of lines.entries()) {
    const at = `${rel}:${index + 1}`;

    if (!COLLISION_ALLOWED.has(rel)) {
      for (const { name, re } of FORBIDDEN_EVERYWHERE) {
        if (re.test(line)) findings.push(`${at}: ${name}`);
      }
    }

    if (ALLOWED.has(rel)) continue;
    for (const { name, re } of RETIRED) {
      if (re.test(line)) findings.push(`${at}: ${name} outside the allowlist`);
    }
  }
}

if (findings.length > 0) {
  console.error("branding guard: FAIL");
  for (const finding of findings) console.error(`  ${finding}`);
  console.error(
    `\n${findings.length} finding(s). A retired name may only appear in the ` +
      `${ALLOWED.size} files listed in scripts/check-branding.mjs, each of which ` +
      `is a migration or reporting concern, and a collision name only in the ` +
      `${COLLISION_ALLOWED.size} documents that record the decision.`
  );
  process.exit(1);
}

console.log("branding guard: PASS (0 retired names outside the allowlist)");
