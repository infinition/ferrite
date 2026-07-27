## What this changes

<!-- One or two sentences. What behaviour is different after this patch. -->

## Checklist

- [ ] `cargo build --release --target x86_64-pc-windows-msvc` succeeds
- [ ] `cargo clippy --all-targets -- -D warnings` is clean
- [ ] `python tools/check_i18n.py` reports no problem

## If this adds a detection rule

- [ ] The rule has a description in every locale under `assets/locales/`
- [ ] Ambiguous names carry a `requires` or `under` constraint
- [ ] The `restore` field names the command that regenerates the artifact
- [ ] The risk level matches: `SAFE` only for artifacts that never hold sources

## If this touches deletion

- [ ] Paths still resolve through the scan index, never from client input
- [ ] Behaviour verified with and without the keep patterns option
