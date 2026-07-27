# Contributing

## Building

```
cargo build --release --target x86_64-pc-windows-msvc
```

The MSVC target matters: `build.rs` locates the Windows SDK resource compiler
to embed the icon and the version metadata. Without it the build still
succeeds, only the executable carries the default icon.

Before opening a pull request:

```
cargo fmt --all
cargo clippy --target x86_64-pc-windows-msvc --all-targets -- -D warnings
python tools/check_i18n.py
```

## Adding a detection rule

Rules live in `src/catalog.rs`, in a single ordered list. The first rule that
matches an entry wins, so specific rules come before generic ones.

A rule needs an identifier, a category, a risk level, the names or patterns it
matches, the command that regenerates the artifact, and the pattern to write
into a `.gitignore`.

```rust
d!("turbo", "js", SAFE, [".turbo"], "npm run build", ".turbo/"),
```

Four points decide whether a rule is good:

**Ambiguous names need a constraint.** `target` is Cargo in one project and
Maven in another, `vendor` is Composer or Go, `bin` is .NET or a directory of
committed scripts. Use `dreq!` to require a marker file next to the candidate,
or `dunder!` to require a parent directory name.

```rust
dreq!("cargo_target", "rust", SAFE, ["target"], ["Cargo.toml"], "cargo build", "target/"),
```

**The risk level is a promise.** `SAFE` means the directory never holds
sources, in any project, and a documented command rebuilds it. When in doubt
use `CHECK`. `DATA` is for re-downloadable payloads where the only loss is
bandwidth.

**Prefer file patterns over directory names for payloads.** There is
deliberately no rule on directories called `models` or `checkpoints`: in a
Django or Rails application, `models/` holds source code. Model weights are
detected by extension instead, which is precise and cannot produce that false
positive.

**Every rule needs a description in every language.** Add
`rules.<id>.desc` to each file under `assets/locales/`. `tools/check_i18n.py`
fails the build otherwise.

## Adding a language

Drop a JSON file into `assets/locales/`, mirroring the structure of `en.json`,
then declare it in the `RAW` table in `src/i18n.rs`. It shows up in the
language picker on its own.

No user facing string may be hardcoded, in the Rust sources or in the front
end. The checker enforces this over the template, the script and the rule
catalogue. Keys assembled at runtime, such as `ignore.tip_` followed by a
status, are listed explicitly in `DYNAMIC_FAMILIES`.

## Touching deletion

Deletion paths are resolved exclusively through the scan index, which maps
`(project, rule, relative path)` to an absolute path. Client input selects
entries from that index and never supplies a path. Keep it that way: it is what
makes a forged request inert.

`is_safe_target` additionally refuses anything outside the scanned workspace,
the workspace root, a project root, and any path containing `.git`.

## Regenerating the icon

```
python tools/make_icon.py
```

This writes `icon.ico`, the interface PNGs and the raw RGBA window icon from a
single drawing.
