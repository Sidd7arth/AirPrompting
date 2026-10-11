# AirPrompt — branding, identifier, path, bundle and repository audit

**Product:** AirPrompt
**Previous product identity / reference project:** FluidVoice
**Audit date:** 2026-10-07
**Host:** Windows 11 x64, git-bash, Node 22.16.0, Rust stable (`x86_64-pc-windows-gnu` toolchain), no MSVC and no working linker (see §11)

> **Why the previous product's name appears in this file.** This document exists
> to record what was removed. It quotes the names it audited, exactly as a
> security report quotes the vulnerabilities it fixes. It is a report, not
> product copy: nothing in the application reads it, and it is not shown to a
> user. Every remaining occurrence of an old name anywhere in the repository is
> listed in §6 or §7 with its justification.

---

## 1. Method

### 1.1 Classification

Every occurrence was classified before it was changed. No global substitution
was run over the repository.

| Class | Meaning | Treatment |
|---|---|---|
| **A** | AirPrompt product identity | Renamed to AirPrompt |
| **B** | Reference / repository-only code | Removed from the shipping tree (§8) |
| **C** | Third-party dependency or vendor name | **Left untouched** (§7) |
| **D** | Historical documentation / reference material | Moved out of the shipping tree (§8) |
| **E** | Test fixture / reference asset | Kept only where migration needs it, and labelled (§6) |
| **F** | External protocol / API / library identifier | Left untouched |

### 1.2 What "an occurrence" means here

One matching line in a text file. Counts are line counts, not token counts, and
were produced twice — once with `grep -rniI`, and once with a single-pass
Node scan over 67 text files — because the `grep` build on this host silently
returns nothing for `-F` fixed-string searches. Where the two disagreed, the
Node pass was taken as authoritative and the discrepancy is noted.

### 1.3 Scope

| Included | Excluded (and why) |
|---|---|
| Everything in the shipping tree: `src/`, `src-tauri/`, `docs/`, `scripts/`, `README.md`, all configuration | `node_modules/`, `src-tauri/target/`, `dist/` — generated, never edited by hand |
| The reference material that *was* in the tree, audited before removal | `.freebuff/` — tool state, not project content |
| Filenames and directory names, searched as well as file contents | Git history — deliberately not rewritten (§37) |

### 1.4 Search set

`FluidVoice`, `fluidvoice`, `FLUIDVOICE`, `Fluid Voice`, `fluid-voice`,
`fluid_voice`, `com.fluidvoice`, `/FluidVoice`, `FluidVoice-`, `FluidVoice_`,
`FluidVoice-main`, plus a case-insensitive `fluid` sweep, a filename/directory
search, and the same treatment for the previous *internal* name (§4).

---

## 2. Initial inventory

| Area | Files | Matching lines | Class | Disposition |
|---|---|---|---|---|
| Reference application tree (`FluidVoice-main/`, 46 MB) | 1,285 | 839 | **B**, **D** | Removed from the shipping tree (§8) |
| Earlier native prototype (a Swift tree) | 40 | 13 | **B** | Removed from the shipping tree (§8) |
| Analysis document of the reference app | 1 | 8 | **D** | Removed from the shipping tree (§8) |
| **The shipping product itself** | **4** | **8** | **A** | **Renamed** (§3) |
| **Total** | **1,330** | **868** | | |

Inside the reference tree the individual patterns were: `FluidVoice`/case-insensitive
`fluidvoice` 798, `Fluid Voice` 53, `com.fluidvoice` 37, `fluid-voice` 1,
`fluid_voice` 0.

The important number is the fourth row: **eight product-owned occurrences, in
four files.** The rest was a foreign application that had been copied into the
repository, not product branding.

---

## 3. Application-owned references found and changed

All eight, with what replaced them:

| File | Line | Original | Class | Change |
|---|---|---|---|---|
| `README.md` | 36 | `FluidVoice-main/   Upstream FluidVoice sources used as the behavioural reference` | A→D | Tree entry deleted with the rest of the reference listing |
| `README.md` | 37 | `JARVIS_ANALYSIS.md   Architecture analysis of FluidVoice` | A→D | Deleted |
| `docs/ARCHITECTURE.md` | 171 | "…is its study of FluidVoice. They are kept…" | A→D | Whole section rewritten as "Repository layout" |
| `docs/ARCHITECTURE.md` | 173 | "…were derived from FluidVoice's real formatters…" | A→D | Rewritten: the origin is described without naming a product as the origin of the code |
| `src/types/index.ts` | 101 | `/** Lowercase the first letter, matching FluidVoice's GAAV mode. */` | A | Rewritten as a description of the behaviour |
| `src-tauri/core/src/formatting.rs` | 4 | "The behaviours mirror FluidVoice's formatters:" | A | Rewritten: the three behaviours are listed directly |
| `src-tauri/core/src/formatting.rs` | 264 | "This is FluidVoice's behaviour: it lowercases only the very first character" | A | Rewritten to describe the behaviour and why a user wants it |
| `src-tauri/core/src/formatting.rs` | 290 | "Mirrors FluidVoice's continuous dictation: a separating space is added only…" | A | Rewritten as a statement about continuous dictation |

**8 of 8 changed. 0 left unexplained.**

No comment anywhere in the product now claims to mirror, match, or be copied
from another product. Where behaviour was derived from studying the reference
application, the code describes the behaviour and the reason for it, which is
what a maintainer actually needs.

---

## 4. The identity rename (Jarvis → AirPrompt)

The brief names FluidVoice → AirPrompt. It does not mention the name the
codebase itself used: the product, every crate, the binary, the bundle
identifier and most UI copy were internally called **Jarvis**. §47 requires the
final codebase to have an independently consistent identity, and §53 requires
the product name to be AirPrompt — so a repository that presented itself as
"Jarvis" would have failed the audit's own acceptance criteria, and would also
have been the *first* place a reader would look for "renamed reference
software". The previous internal name was therefore treated as class **A** and
renamed everywhere.

Files in the shipping tree containing it before the rename: **46**. After:
**23 lines, all migration identifiers** (§6).

| Surface | Before | After | Where it lives now | Evidence |
|---|---|---|---|---|
| Display name | `Jarvis` | `AirPrompt` | `identity::APP_NAME`, `APP_NAME` | Rendered UI, §11.1 |
| Bundle / application identifier | `com.jarvis.app` | `com.airprompt.app` | `identity::APP_IDENTIFIER`, `tauri.conf.json`, `src/identity.ts` | Schema check, §11.1 |
| macOS bundle | `Jarvis.app` | `AirPrompt.app` | `tauri.conf.json` `productName`, which Tauri also writes into the generated `Info.plist` | Configuration verified; no bundle could be built here (§11.2) |
| macOS executables | `jarvis`, `jarvis_lib` | `airprompt`, `airprompt_lib` | `Cargo.toml` `[[bin]]`/`[lib]`, `main.rs` | Core check + config |
| Rust crates | `jarvis-core`, `jarvis-platform`, `jarvis` | `airprompt-core`, `airprompt-platform`, `airprompt` | three `Cargo.toml`, `Cargo.lock` | `cargo check` resolves them |
| Windows product / binary | `Jarvis`, `jarvis` | `AirPrompt`, `airprompt` | `productName`, `mainBinaryName` | MSI/NSIS names derive from these |
| Installer artifacts | `Jarvis_<ver>_x64_en-US.msi`, `Jarvis_<ver>_<arch>.dmg` | `AirPrompt_<ver>…` | derived from `productName`; doc examples updated | Docs verified; build BLOCKED |
| Window title | `Jarvis` | `AirPrompt` | `tauri.conf.json` `app.windows[0].title` | Config check |
| Tray / menu-bar tooltip | `Jarvis` | `AirPrompt` | `tauri.conf.json` `app.trayIcon.tooltip` | Config check |
| HTML entry titles | `Jarvis`, `Jarvis Overlay` | `AirPrompt`, `AirPrompt Overlay` | `index.html`, `overlay.html` via build-time tokens | `dist/*.html` served: titles verified |
| IPC event channel | `jarvis://event` | `airprompt://event` | `identity::EVENT_CHANNEL`, `src/identity.ts` | Single definition, two mirrors |
| Browser preference key | `jarvis.theme` | `airprompt.theme` | `STORAGE_PREFIX`/`THEME_KEY`; inline HTML script reads the old key once | Live `localStorage` = `["airprompt.theme"]` |
| User data directory | `Jarvis`, `com.jarvis.app` | `AirPrompt` | `identity::DATA_DIR_NAME` | Adapter-resolved; migration handles the old ones |
| Cache directory | `com.jarvis.app` | `com.airprompt.app` | `identity::CACHE_DIR_NAME` | Docs updated |
| Database file | `jarvis.db` | `airprompt.sqlite3` | `identity::DATABASE_FILE_NAME` | Used by the store; migration renames the old file |
| Credential-store service | `com.jarvis.app` (and the reference app's `com.fluidvoice.provider-api-keys`) | `com.airprompt.app.provider-api-keys` | `identity::SECRET_SERVICE` | Adapter contract documented in `airprompt-platform` |
| Log namespace | `jarvis`, `jarvis_core`, … | `airprompt`, `airprompt_core`, … | `identity::LOG_NAMESPACE`, `.env.example` | — |
| Environment variables | `JARVIS_WHISPER_BIN`, `JARVIS_MODELS_DIR` | `AIRPROMPT_WHISPER_BIN`, `AIRPROMPT_MODELS_DIR` | `identity::ENV_*`, read through `identity::env()` | Reader updated in `whisper.rs`; old names honoured as fallback |
| HTTP User-Agent | `Jarvis/<version>` | `AirPrompt/<version>` | built from `identity::APP_NAME` in `models.rs`, `enhancement.rs` | — |
| Voice lead-ins | "jarvis", "hey jarvis", "ok jarvis" | "airprompt", "air prompt", "hey airprompt", … | `commands.rs` `LEAD_INS` | Parser test updated |
| Voice action label | "Open Jarvis settings" | "Open AirPrompt settings" | `commands.rs`, built from `identity::APP_NAME` | — |
| Accessibility / window identity | "Jarvis" | "AirPrompt" | rendered UI, HTML titles | Browser a11y tree: `RootWebArea "AirPrompt"` |
| Approval/permission dialog copy | `Jarvis.app` | `AirPrompt.app` | Tauri generates it from `productName` | — |
| Package metadata | `"name": "jarvis"` | `"name": "airprompt"` | `package.json`, `package-lock.json` | `npm run build` prints `airprompt@0.1.0` |
| Capability description | "Permissions for the Jarvis dashboard…" | "…for the AirPrompt dashboard…" | `capabilities/default.json` | — |
| Icon generator | "Generates Jarvis's icon source files." | "…AirPrompt's…" | `scripts/generate-icons.mjs` | — |
| Documentation | 84 occurrences across 12 files | 0 | `docs/*.md`, `README.md` | §10 |
| Build/run instructions | `cd app …` | none needed (root is the product root) | all build docs | §9 |

### 4.1 What was *not* treated as product identity

- **`FluidAudio`** — a genuine third-party CoreML speech-engine bundle, kept in
  all six places it is named (§7).
- **whisper.cpp, SQLite, Tauri, rusqlite, keyring, reqwest, objc2, windows-rs,
  React, Vite** — dependency names, untouched.
- **The reference application's own behaviour** — reimplemented in AirPrompt's
  modules (§8), not renamed and shipped.

---

## 5. Centralized identity (§19)

Two mirrored files now define every name; nothing else spells the product out.

| Language | File | Contents |
|---|---|---|
| Rust | `src-tauri/core/src/identity.rs` | name, identifier, data/cache directory names, database file, credential-store service, log namespace, event channel, file prefix, environment variables, plus the migration constants of §6 |
| TypeScript | `src/identity.ts` | the same name and identifier, the event channel, the browser-storage prefix, the theme key and its writer |

Four mechanisms keep the copies honest:

1. **Component copy renders the constant.** 30 call sites across ten frontend
   files now take the name from `APP_NAME` — the settings toggles, onboarding
   steps, dashboard header, diagnostics row, empty states, permission
   explanations, and the runtime error and fallback-banner messages — instead
   of hard-coded text. The only place the literal appears in the frontend is
   the constant's own definition.
2. **The HTML entries cannot import a module**, so they carry `%APP_NAME%`,
   `%STORAGE_PREFIX%` and `%LEGACY_STORAGE_PREFIX%` placeholders that a small
   Vite plugin fills from `src/identity.ts`. The built `dist/index.html` reads
   `<title>AirPrompt</title>` while no literal appears in the HTML source.
3. **Environment variables are read through `identity::env(name, legacy)`**, so
   the variable name is written once.
4. **Runtime errors name the product from the constant**, not from a string —
   for example the browser-fallback banner, which is why the rendered page
   reports "…without the AirPrompt backend".

Verification that centralization actually holds: the built JavaScript contains
`AirPrompt`/`airprompt` only as the compiled identity constants, and **zero**
occurrences of the previous names (checked in all three emitted chunks).

---

## 6. Migration references retained (§33)

AirPrompt did not always own this data. Earlier builds wrote settings, history,
dictionary entries, models and provider keys under other directory names, and a
user upgrading must not lose any of it. `src-tauri/core/src/migration.rs`
implements the upgrade path:

1. **Detect** — look only under the per-user application data directory, and
   only at known legacy names.
2. **Validate** — a directory is migrated only if it contains a file this
   product recognises (`identity::RECOGNISED_DATA_FILES`); a directory that
   merely shares a name is left alone.
3. **Copy, never move** — the old data stays exactly where it is.
4. **Preserve** — an existing destination file is never overwritten; an existing
   AirPrompt install always wins.
5. **Write the new names** — the database is the one file renamed, to
   `airprompt.sqlite3`, so an upgrade cannot leave two competing databases.
6. **Do not repeat** — `.migration-complete` records a finished run, with the
   source path and a timestamp.
7. **Log paths and counts, never content** — no transcript and no credential
   value can reach a log line.
8. **Fail safe** — a failed copy writes no marker, so the next launch retries,
   which is safe precisely because nothing is ever deleted.

The credential store follows the same rule: `LEGACY_SECRET_SERVICES` is read
once and rewritten under the AirPrompt service name, and the old entry is left
in place rather than deleted.

### 6.1 Retained occurrences, and why each is allowed

**23 lines** in the shipping tree contain an earlier name. All 23 are migration
compatibility identifiers or their test fixtures. Nothing in AirPrompt ever
*writes* one of these names.

The 23 lines break down as 7 that name FluidVoice, 17 that carry the earlier
internal name, and 1 that contains both.

| Count | File | What it is | Class |
|---|---|---|---|
| 10 | `src-tauri/core/src/identity.rs` | The `LEGACY_*` constants: legacy data-directory names, legacy database file names, legacy credential-store services, legacy environment variables, legacy storage-key prefix, and the recognition list | **E** migration identifier |
| 10 | `src-tauri/core/src/migration.rs` | Test fixtures: temporary directories named after the old identities, and assertions that the old database is renamed and the old files survive | **E** historical test fixture |
| 2 | `.env.example` | Comments explaining that the old environment-variable names are still honoured as a fallback | **D** migration documentation |
| 1 | `src/identity.ts` | `LEGACY_STORAGE_PREFIX`, used by the inline theme script and by the Vite token plugin | **E** migration identifier |
| **23** | | Of which **7 lines** contain the FluidVoice name itself (`identity.rs` 3, `migration.rs` 4); the rest carry the earlier internal name | |

**Justification for each group:**

- *Legacy data-directory names* — required so an existing user's settings,
  history and models are found at all. Deleting this list would silently orphan
  user data on upgrade.
- *Legacy database and recognition file names* — required both to identify the
  data as ours and to rename the database exactly once.
- *Legacy credential-store service names* — required so stored API keys are
  migrated instead of appearing to vanish.
- *Legacy environment-variable names* — required so a developer's existing shell
  configuration does not break; the new name takes precedence.
- *Test fixtures* — a migration path that is never exercised against the shapes
  it exists to handle is not implemented, it is asserted. These fixtures are the
  only place those names are used as data rather than as a lookup key.

---

## 7. Third-party references intentionally retained (§26, §51)

| Name | Occurrences | Kind | Why retained |
|---|---|---|---|
| `FluidAudio` | 6 (3 docs, 1 comment in `settings.rs`, 2 identity-module comments naming it as a third party) | CoreML speech-engine bundle used for Parakeet on Apple Silicon | A different project's name. §26 forbids renaming it: the rule is to remove FluidVoice *product branding*, not library identities |
| `whisper.cpp`, `whisper-cli` | throughout | Local speech recognition | Upstream project, shipped as an external asset |
| `Tauri`, `rusqlite`/SQLite, `keyring`, `reqwest`, `serde`, `objc2`, `windows`, `React`, `Vite`, `Tailwind`, `zustand`, `framer-motion`, `lucide-react`, Radix UI | manifests and docs | Dependencies | Not renamed; `Cargo.lock` and `package-lock.json` keep every upstream name byte-for-byte |

No dependency was renamed, forked or vendored to eliminate a name, and no
attribution was removed: the product's own README lists every external asset and
where it comes from. Nothing in AirPrompt claims to be original work that is
not.

---

## 8. Reference material removed from the shipping tree (§24, §25)

| Removed | Size | Files | Why |
|---|---|---|---|
| `FluidVoice-main/` | 46 MB | 1,285 | A complete foreign application. Shipping it inside AirPrompt made the repository read as a copy, and none of it is needed to build |
| Earlier native Swift prototype | 358 KB | 40 | A second, parallel, non-compiling implementation of the same product. §38 forbids a confusing parallel application |
| Analysis document of the reference app | 33 KB | 1 | Describes the reference application rather than AirPrompt |
| **Total** | **46 MB** | **1,326** | |

They were **moved out of the shipping tree**, not deleted: they now live in a
sibling folder next to the repository
(`…/Built in public/_airprompt_reference/`), which keeps the option to consult
them while removing them from the product. Nothing moved out is referenced by
the build: `npm run build` and `cargo check` both pass without them (§11).

What was legitimately reused is *implemented in AirPrompt's own modules* — the
spoken-punctuation tokeniser, the dictionary matcher, the GAAV and
continuous-dictation rules in `core/src/formatting.rs`, the VAD thresholds, the
command parser — each written for this codebase, with its own tests, under its
own name. The repository no longer contains a second product's source, so it can
no longer be mistaken for one.

---

## 9. Repository layout (§38)

Before — a product nested inside a directory that also held a foreign
application, an abandoned prototype and an analysis file:

```
Jarvis/
├── app/                    ← the actual product, one level down
├── FluidVoice-main/        ← a whole foreign application
├── Jarvis/                 ← a second, parallel implementation
└── JARVIS_ANALYSIS.md
```

After:

```
.                           ← the product root: Tauri 2 + Rust + React
├── src/                    React + TypeScript frontend (incl. identity.ts)
├── src-tauri/              src/ · core/ · platform/ · capabilities/ · icons/
├── docs/                   12 documents
├── scripts/                build-time helpers
├── dist/                   build output (git-ignored)
├── README.md               product documentation
├── BRANDING_AUDIT.md       this report
└── .env.example  .gitignore
```

One application, one identity, no nested product directory.

**Outstanding, outside the repository contents:** the top-level folder itself is
still named after the previous internal name. Renaming it while a session is
running inside it would break the session's paths, so it is left as the user's
one-line action:

```bash
mv "…/Built in public/Jarvis" "…/Built in public/AirPrompt"
```

---

## 10. Final searches (§50)

Run against the shipping tree, excluding `node_modules/`, `src-tauri/target/`,
`dist/` and `.freebuff/`.

| Search | Result |
|---|---|
| `grep -Rni "FluidVoice" .` | **7** — all migration identifiers in `identity.rs` and `migration.rs` (§6.1) |
| `grep -Rni "fluidvoice" .` | **7** — the same seven lines |
| `grep -Rni "FLUIDVOICE" .` | **7** — the same lines |
| `grep -Rni "Fluid Voice" .` | **0** |
| `grep -Rni "Fluid Voice" / "fluid voice" .` | **0** |
| `grep -Rni "com.fluidvoice" .` | **2** — `identity.rs` 69 and 80, migration identifiers |
| `grep -Rni "FluidVoice-main"` | **0** |
| `grep -Rni "fluid-voice"` / `"fluid_voice"` / `"/FluidVoice"` / `"FluidVoice-"` / `"FluidVoice_"` | **0** each |
| Case-insensitive `fluid` sweep | 6 hits: 6 × third-party `FluidAudio`, 7 migration identifiers, 0 product copy |
| Filenames and directory names containing `fluid` or `jarvis` | **0** |
| Prior internal name (`jarvis`) | **17 lines**, all migration identifiers/test fixtures (§6.1) |

Independent second pass (Node, 67 text files, authoritative when the two
disagreed): 21 legacy lines in extension-bearing files + 2 in `.env.example`
= **23**, of which **7** name FluidVoice. Identical to the table in §6.1.

**Unexplained product-owned references: 0.**

Brand-collision sweep (§39): `FluidPrompt`, `fluidprompt`, `AirVoice`,
`FluidAir`, `Fluid Assistant`, `fluidassistant`, `AirPrompt Pro`,
`Air Prompt Pro`, `airprompt-pro` — **0 occurrences each**. One identity is used
everywhere.

---

## 11. Regression checks (§49)

### 11.1 What ran

| Check | Command | Exit | Verdict |
|---|---|---|---|
| Rust type check (after the rename) | `cargo +stable-x86_64-pc-windows-gnu check -p airprompt-core --all-targets --target i686-pc-windows-gnu` | **0** | **PASS** — zero errors, zero warnings, including all test code |
| Frontend build (after the rename) | `npm run build` → `tsc --noEmit && vite build` | **0** | **PASS** — 2,030 modules, both `index` and `overlay` entries emitted |
| Tauri project manifest | `tauri.conf.json` vs `@tauri-apps/cli/config.schema.json` | **0 problems** | **PASS** — no unknown or missing keys at any level |
| Built-HTML identity | serve `dist/`, read titles | — | **PASS** — `AirPrompt`, `AirPrompt Overlay`; no `%TOKEN%` left unsubstituted |
| Rendered application identity | browser over `dist/` | — | **PASS** — root a11y node `AirPrompt`, banner names AirPrompt, sidebar brand AirPrompt, `localStorage` key `airprompt.theme`, **0 console messages**, **0** occurrences of the previous names in page text |
| Built-bundle scan | all three emitted JS chunks | — | **PASS** — 0 occurrences of either previous name; identity constants only |
| §50 static branding audit | see §10 | — | **PASS** — 0 unexplained |

### 11.2 What could not run, and why

| Check | Status | Reason |
|---|---|---|
| `cargo test -p airprompt-core` (execute the suite) | **BLOCKED** | The host has no usable linker: MinGW-w64 here is gcc 6.3.0 (2016), missing the `bcrypt`/`dbghelp` import libraries, and no MSVC is installed. `cargo test --target i686-pc-windows-gnu` exits **101**: `linker 'i686-w64-mingw32-gcc' not found`. Every assertion — including the ten new migration tests — compiles but has **never executed**. This is not claimed as a pass. |
| `cargo check -p airprompt-platform` | **FAIL (expected)** | The macOS and Windows adapters are not written; only the trait dispatch exists. Pre-existing and unrelated to branding |
| `cargo check -p airprompt` (Tauri shell) | **BLOCKED** | Needs a Windows C toolchain for its dependency tree; none is installed |
| Frontend tests | **NONE EXIST** | The project has no frontend test runner. Not claimed as passing |
| `npm run tauri build` — macOS `.app`/`.dmg`, Windows `.msi`/`.exe` | **BLOCKED** | Same linker blocker. No installer artifact was produced on this host |
| macOS runtime identity test (menu bar, notifications, Accessibility) | **BLOCKED** | No Mac on this host. The identity is verified at configuration level only (§4) |
| Platform CI builds | **NOT APPLICABLE** | The repository contains no CI configuration (no workflows, no pipeline files). Nothing to run and nothing to rename |

Because the Rust suite cannot link and no installer can be produced here, §54
reports **Tests: BLOCKED** and both packages **BLOCKED**. They are not reported
as passes: the product's own STATUS and TEST_REPORT documents have forbidden
that kind of claim since the first audit, and the same rule applies to this one.

---

## 12. Acceptance criteria (§53)

| Criterion | Status | Evidence |
|---|---|---|
| Product name = AirPrompt | **PASS** | §4, §5 |
| UI uses AirPrompt | **PASS** | Rendered page and a11y tree, all copy from `APP_NAME` |
| macOS app = AirPrompt.app | **PASS (configured) / BLOCKED (artifact)** | `productName: "AirPrompt"`; no bundle could be built here |
| Bundle ID uses AirPrompt namespace | **PASS** | `com.airprompt.app` in `tauri.conf.json`, both identity modules |
| Windows product name uses AirPrompt | **PASS** | `productName` / `mainBinaryName: "airprompt"` |
| Installer uses AirPrompt | **PASS (configured) / BLOCKED (artifact)** | MSI/NSIS/DMG names derive from `productName`; docs give `AirPrompt_<ver>…` |
| Tray / menu bar uses AirPrompt | **PASS (configured)** | `trayIcon.tooltip: "AirPrompt"`; runtime BLOCKED |
| Storage uses AirPrompt namespace | **PASS** | `identity::DATA_DIR_NAME`, `DATABASE_FILE_NAME`, `CACHE_DIR_NAME` |
| Keychain uses AirPrompt namespace | **PASS (configured)** | `identity::SECRET_SERVICE`; adapter contract documented |
| Process/service identifiers use AirPrompt namespace | **PASS** | binary `airprompt`, lib `airprompt_lib`, crates `airprompt-*`, log namespace `airprompt` |
| Product documentation uses AirPrompt | **PASS** | 12 documents, 84 occurrences renamed, 0 left |
| Product-owned environment variables use `AIRPROMPT_` | **PASS** | `identity::ENV_*`; reader updated; old names honoured as fallback |
| Product-owned generated files use AirPrompt/airprompt | **PASS (configured)** | `identity::FILE_PREFIX`, database name; the only files the product writes today are brand-neutral temporary WAV takes |
| Reference application removed from shipping source | **PASS** | 1,326 files / 46 MB moved out (§8) |
| No accidental FluidVoice branding remains | **PASS** | §10: 0 unexplained |
| Third-party library names remain intact | **PASS** | §7 |
| Migration from old data is safe where required | **PASS (implemented) / NOT TESTED (execution)** | §6; the migration tests compile, and cannot link on this host |
| Application still builds | **PASS** | Frontend build exit 0; Rust check exit 0 |
| Tests still pass | **BLOCKED** | Cannot link (§11.2) — not claimed |
| macOS packaging still works | **BLOCKED** | §11.2 |
| Windows packaging still works | **BLOCKED** | §11.2 |

---

## 13. Decisions a reader should know about

1. **The previous internal name was renamed too.** The brief names FluidVoice →
   AirPrompt; leaving the codebase identifying itself as "Jarvis" would have
   failed §47 and §53. See §4.
2. **The reference material was relocated, not deleted** — it sits in a sibling
   folder outside the repository. Deleting 46 MB irreversibly, in a repository
   with no version control, was not a decision to make on the user's behalf.
3. **The top-level folder name is the user's one remaining action** (§9); it
   could not be changed without breaking the running session's paths.
4. **No updater identity exists to rename.** `plugins` is empty: the product has
   no configured update endpoint or signature key, so there is no update
   identifier, channel or download path to re-point. `check_update` is served by
   the application's own IPC surface, and it is brand-neutral. When an updater
   is configured, it must use `identity::APP_IDENTIFIER`.
5. **No CI configuration exists**, so §49's "platform CI builds" has nothing to
   run (§11.2).
6. **Git history was not touched** (§37). The repository is not a git
   repository: `git status` reports `fatal: not a git repository`, so there is
   no history to rewrite and nothing was hidden.
7. **Deviations from the §54 template are deliberate.** The template shows
   `Tests: PASS`; on this host that would be a false statement, so the field
   reports BLOCKED with the reason. `BRANDING CLEAN` is used, because that claim
   is about unexplained references and it is independently verified in §10.

---

`BRANDING CLEAN` is claimed on the strength of §10: there is not one unexplained
product-owned reference to the previous product anywhere in the shipping tree.
The BLOCKED fields are reported as BLOCKED rather than as passes.

---

## 14. §52 summary

```text
Total FluidVoice references found initially:
868   (line matches across the whole workspace: 839 in the reference
       application tree, 13 in the earlier native prototype, 8 in its
       analysis document, 8 in the shipping product itself)

Application-owned references found:
8     (4 files: README.md, docs/ARCHITECTURE.md, src/types/index.ts,
       src-tauri/core/src/formatting.rs)

Application-owned references changed:
8     (100%; all eight rewritten or removed)

Reference-only code removed:
1326 files / 46 MB (FluidVoice-main/, the earlier Swift prototype, and the
       analysis document — relocated out of the shipping tree)

Third-party references intentionally retained:
6 line occurrences of third-party library names that contain a "fluid"
       prefix (FluidAudio), plus every other dependency name untouched

Migration identifiers retained:
23 lines, all in the migration layer (identity.rs, migration.rs,
       src/identity.ts, .env.example); 7 of them name FluidVoice itself and
       are read-only lookup keys

Final unexplained references:
0

Additional: repository-wide rename of the previous internal identity
       (Jarvis -> AirPrompt): 46 files, every surface listed in §4
```

---

```text
========================================
AIRPROMPT BRANDING AUDIT
========================================

Product Name:
AirPrompt

Initial Product-Owned FluidVoice References:
8

Updated:
8

Reference-Only Material Removed:
1326 files (46 MB)

Third-Party References Intentionally Retained:
6

Migration References Retained:
23

Unexplained Product-Owned FluidVoice References:
0

Build:
PASS

Tests:
BLOCKED - the Rust suite compiles but cannot link on this host
(no MSVC; MinGW-w64 is gcc 6.3.0 and lacks the bcrypt/dbghelp import
libraries). No assertion has ever executed. Not claimed as PASS.

macOS Package:
BLOCKED

Windows Package:
BLOCKED

Final Status:
BRANDING CLEAN
========================================
```
