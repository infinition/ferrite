# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-07-27

First release.

### Added

- Desktop window built on `tao` and `wry`, rendering through WebView2. The
  local server, the interface and the language catalogues are embedded in a
  single executable.
- Project discovery by marker file, over a configurable depth of 1 to 6 levels.
  A directory recognised as a project is not searched deeper.
- 120 detection rules across 18 ecosystems, with three risk levels: safe,
  review, and data.
- Ambiguous names gated on a sibling marker, so a Cargo `target` is told from a
  Maven `target`, and a Composer `vendor` from a Go `vendor`.
- Size and file count per artifact and per project, with a reclaimable ratio
  gauge, per project bars and per artifact bars.
- Two level expansion: project, then artifact, then the individual locations
  with their own size and coverage flag.
- `.gitignore` coverage computed with `git check-ignore`, summarised on the
  project header without expanding it, and fixable in one click.
- A distinct state for paths already in the git index, which no `.gitignore`
  pattern can affect until `git rm -r --cached` runs. Reported explicitly
  because otherwise adding the pattern looks like it did nothing.
- Tracked file detection with `git ls-files`, surfaced as a warning before
  deletion.
- Selective cleaning that preserves `*.exe` files, along with the directories
  needed to reach them.
- Repository compaction through `git gc` per project.
- Full internationalisation, French and English, covering the interface and the
  backend messages, with `tools/check_i18n.py` enforcing coverage.

### Security

- Deletion targets resolve only through the scan index. A forged selection is
  rejected.
- Paths outside the scanned workspace, the workspace root, project roots and
  anything under `.git` are refused.

[1.0.0]: https://github.com/infinition/ferrite/releases/tag/v1.0.0
