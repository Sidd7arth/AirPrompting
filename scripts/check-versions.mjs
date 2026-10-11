#!/usr/bin/env node
/**
 * Version consistency gate (spec §51).
 *
 * The version that a user sees comes from four places that can silently drift:
 * the frontend package manifest, the Tauri application config (which names the
 * installer and the update feed), the Rust shell crate, and the core crate.
 * A release where the installer says 0.1.0 and the app reports 0.2.0 is a
 * support nightmare and breaks update comparisons, so this fails the build.
 *
 * Usage:  node scripts/check-versions.mjs
 * Exit:   0 consistent, 1 inconsistent
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(fileURLToPath(new URL(".", import.meta.url)), "..");

function read(rel) {
  return readFileSync(join(ROOT, rel), "utf8");
}

/** First `version = "..."` in a Cargo.toml `[package]` section. */
function cargoVersion(rel) {
  const match = read(rel).match(/\[package\][\s\S]*?\nversion\s*=\s*"([^"]+)"/);
  return match ? match[1] : null;
}

const sources = [
  ["package.json", JSON.parse(read("package.json")).version],
  ["src-tauri/tauri.conf.json", JSON.parse(read("src-tauri/tauri.conf.json")).version],
  ["src-tauri/Cargo.toml", cargoVersion("src-tauri/Cargo.toml")],
  ["src-tauri/core/Cargo.toml", cargoVersion("src-tauri/core/Cargo.toml")],
  ["src-tauri/platform/Cargo.toml", cargoVersion("src-tauri/platform/Cargo.toml")],
];

const missing = sources.filter(([, version]) => !version);
if (missing.length > 0) {
  console.error("version gate: FAIL — no version found in:");
  for (const [file] of missing) console.error(`  ${file}`);
  process.exit(1);
}

const unique = [...new Set(sources.map(([, version]) => version))];

for (const [file, version] of sources) {
  console.log(`  ${version}  ${file}`);
}

if (unique.length !== 1) {
  console.error(
    `\nversion gate: FAIL — ${unique.length} different versions: ${unique.join(", ")}. ` +
      `Set one version in all five files (spec §51).`
  );
  process.exit(1);
}

console.log(`version gate: PASS (all five sources report ${unique[0]})`);
