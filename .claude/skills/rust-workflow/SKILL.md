---
name: rust-workflow
description: Rust workflow for this Bevy port — the cargo gate sequence (check → clippy → fmt → test), edition/MSRV rules, error-handling policy, and Bevy-specific borrow-checker guidance. Use when writing, reviewing, or debugging any Rust code, before every commit, and when clippy or the borrow checker complains.
---

# Rust Workflow — Port Skill

## The gate (run before every commit)

Run these in order. Each step is cheaper than the next; fix failures before
moving on.

```bash
cargo check
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo test
```

- `cargo check` — fast type/borrow pass, no codegen.
- `cargo clippy --all-targets -- -D warnings` — the hard gate. Warnings are
  errors. Treat clippy as the idiom oracle: if it flags your code, the
  idiomatic fix is usually the right one, not an `#[allow]`. An `#[allow]`
  needs a one-line comment saying why the lint is wrong for that case.
- `cargo fmt --check` — formatting must be clean. Run `cargo fmt` to fix.
- `cargo test` — full test suite.

Run one cargo command at a time — two concurrent runs block each other on
the target-directory lock.

## Edition and MSRV

- This project targets **edition 2024** (check `Cargo.toml` — if it still says
  2021, the upgrade is pending; write new code to 2024 idioms anyway). Check
  `rust-version` (MSRV) field and read the number there — do not assume your
  local toolchain's version. A language feature stabilized after the MSRV
  does not exist for this project, however new your installed Rust is.
- The MSRV moves with Bevy: upgrading `bevy` can raise the floor. After any
  Bevy upgrade, re-check `rust-version`.

## Error handling policy

- **Application/binary code → `anyhow`.** Context-rich errors with `?`:

```rust
use anyhow::{Context, Result};

fn load_config(path: &Path) -> Result<Config> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config from {}", path.display()))?;
    Ok(toml::from_str(&text).context("Failed to parse config")?)
}
```

- **Library/shared code → `thiserror`.** Typed errors callers can match on:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SaveError {
    #[error("Save slot {slot} not found")]
    NotFound { slot: u8 },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

- **No `.unwrap()` / `.expect()` in game code.** They panic the whole game.
  Use `let ... else` for early return, or propagate with `?`. Unwrap is only
  acceptable in tests and in one-line prototypes you will delete.

## Idioms for this port

- **Newtypes for game IDs.** The port juggles many ID spaces (villager IDs,
  item IDs, acre IDs). Wrap them so the compiler catches mixups:

```rust
pub struct VillagerId(u32);
pub struct ItemId(u16);
```

- **Parse, don't validate, at boundaries.** Config files, save data, and
  imported assets get parsed into typed structs once at load; game systems
  then trust the types instead of re-checking strings and numbers.
- **Prefer `&str` params, `String` only when owned.** Use
  `Vec::with_capacity(n)` when the size is known. Avoid `clone()` in hot
  paths — restructure to borrow.
- **Combinators over match** when concise: `.map()`, `.and_then()`,
  `.unwrap_or()`, `.as_deref()`.

## Borrow checker × Bevy ECS

Most borrow errors in this codebase come from Bevy queries, not plain Rust:

- Two queries (or two systems) cannot mutably borrow the same component
  type. Split the work or use `get_many_mut([e1, e2])` for several entities
  under one query.
- Filter with `Changed<T>` / `Added<T>` instead of scanning everything.
- Observers must not mutate the `World` directly — use `commands`.
- When the borrow checker and a Bevy system disagree, the fix is almost
  always *narrower queries*, not `unsafe` or `Arc<Mutex<..>>` wallpaper.

## Unsafe

This project forbids `unsafe_code` unless there is a documented, reviewed
reason. If you think you need `unsafe`, stop and ask — there is almost
always a safe Bevy/Rust pattern (the borrow-checker section above covers
the common cases).
