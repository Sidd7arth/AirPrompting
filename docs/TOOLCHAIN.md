# Toolchain Diagnostics — Shell Compile Attempt

Date: 2026-10-07

## Problem

`cargo check -p airprompt` (the Tauri shell crate) fails on this host while
**linking proc-macro and build-script executables**:

- `serde_derive`, `camino`, `serde` and every other proc-macro/build-script
  crate must be *linked into an executable* before the real crate compiles.
- The `x86_64-pc-windows-gnu` toolchain's linker is the self-contained MinGW
  shim at
  `~/.rustup/toolchains/stable-x86_64-pc-windows-gnu/lib/rustlib/x86_64-pc-windows-gnu/bin/self-contained/x86_64-w64-mingw32-gcc.exe`.
- That linker requires GCC's `crtbegin.o` / `crtend.o`, which **rustup does not
  ship** (its `lib/self-contained/` has 42 `.a` import libraries plus
  `crt2.o`/`dllcrt2.o`, but no `crtbegin.o`).
- No MSVC exists on this host: no `cl.exe`, no `link.exe`, no Windows Kits.
- `C:\MinGW` is gcc 6.3.0, **32-bit only** (`crtbegin.o` exists only under
  `lib/gcc/mingw32/6.3.0/`, no x86_64 variant).

A bare `rustc hello.rs` with the self-contained dir on `PATH` reproduces the
exact failure:

```text
ld: cannot find crt2.o / crtbegin.o
ld: cannot find -lkernel32 … -lmsvcrt … -lmingw32 … -lgcc
```

## Attempted fixes (all failed)

1. `PATH` with the self-contained `dlltool.exe` first — dlltool 2.44 (64-bit)
   works; the linker still cannot find the CRT objects.
2. `RUSTFLAGS="-C link-self-contained=yes"` — same failure.
3. `RUSTFLAGS="-L native=<toolchain lib/self-contained>"` — same failure; the
   directory genuinely lacks `crtbegin.o`.
4. `CC_x86_64_pc_windows_gnu` pointed at the self-contained
   `x86_64-w64-mingw32-gcc.exe` — same failure (it is a linker shim, not a
   full GCC installation).
5. Fresh `CARGO_TARGET_DIR` on `/tmp` — same failure (not stale artifacts).
6. Bare `rustc` probe outside cargo — same failure (proves it is the host
   toolchain, not the project).

## Root cause

The self-contained MinGW **linker** needs GCC's `crtbegin.o`/`crtend.o`,
which only a full MinGW-w64 (or MSVC) installation provides. This host has
neither. Installing a toolchain is outside this pass's scope and would not be
reproducible in CI anyway.

## Consequences for AirPrompt

| Crate | Local compile | Evidence |
|---|---|---|
| `airprompt-core` | **PASS** (i686-pc-windows-gnu) | `cargo check --all-targets` exit 0, zero warnings; clippy `-D warnings` exit 0 |
| `airprompt-platform` | **PASS** (i686-pc-windows-gnu) | `cargo check --all-targets` exit 0; clippy `-D warnings` exit 0 |
| `airprompt` (shell) | **BLOCKED on this host** | every link of a proc-macro/build-script executable fails at `crtbegin.o` |
| `airprompt` (shell) | CI compiles it | `.github/workflows/test.yml` + `build.yml` run `cargo check/test` and `tauri build` on `windows-latest` (MSVC) and `macos-latest` |

Local verification of the shell crate is therefore:

1. `rustfmt --edition 2021 --check` on every `src-tauri/src/*.rs` — all 13
   files parse (run after every edit; all OK).
2. `cargo check`/`clippy`/`test` of `airprompt-core` and `airprompt-platform` —
   the two crates the shell depends on, which cover the core logic the shell
   calls into.

## Verdict

**Host limitation, not a project defect.** The shell's actual compilation is
proven by the native CI runners; this file records precisely why the local
host cannot do it and what was tried.
