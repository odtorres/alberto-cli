# alberto-cli — TUI v2 (Phase 4 batch, v0.3.0)

**Date:** 2026-07-12
**Status:** Approved by Oscar (design presented 2026-07-06, "go ahead")
**Parent spec:** 2026-07-04-alberto-cli-improvements-design.md (Phase 4 backlog items 1, 2, 4, 5)

## Scope

Four TUI features, no backend changes, released as v0.3.0:

1. Fuzzy filter (`/`)
2. Auto-refresh toggle (`a`)
3. Image preview (PNG/JPEG)
4. Configurable download directory

Explicitly out: TUI upload (backlog 3), bulk upload (6), `--watch` (7), `node
search` (8, blocked on backend).

## Approach

Incremental on the existing `src/tui.rs` (615 lines), with one structural
addition: an explicit `enum Mode { Browse, Filter, Preview }` replacing the
implicit two-mode state (`preview: Option<Preview>` stays as data; `Mode`
drives key routing). No external deps beyond enabling the `image` crate's
`jpeg` feature. Rejected: event-loop rework (churn without payoff) and
`fuzzy-matcher`/`ratatui-image` crates (a subsequence matcher is ~10 lines;
the half-block renderer already draws images anywhere).

## Feature 1 — Fuzzy filter (`/`)

- `/` in Browse enters Filter mode: the status bar becomes an input line
  (`🔎 <texto> · Esc limpiar · Enter aplicar`).
- Matching: case-insensitive **subsequence** (fzf-style: `fac` matches
  `factura_2026.pdf`). Pure function, unit-tested:
  `fn fuzzy_match(filter: &str, name: &str) -> bool` — empty filter matches
  everything.
- While typing: list shows only matching nodes; ↑↓ move within the filtered
  view; Enter applies the filter and returns to Browse (selection preserved);
  Esc clears the filter and shows all; Backspace edits the filter text.
- In Browse with an active filter: the list stays filtered (title shows
  `🔎 texto`); Esc clears it; entering a child or going up (Backspace) resets
  the filter.
- State: `filter: String` on `Level`; rendering and selection operate over
  `filtered_indices()` (indices into `nodes`).

## Feature 2 — Auto-refresh toggle (`a`)

- Event loop switches from blocking `event::read()` to `event::poll(1s)`;
  on poll timeout with auto-refresh enabled and ≥5 s since the last fetch,
  call the existing `refresh()` (Task 11: preserves cursor clamp).
- `a` toggles; status shows `auto ⟳ 5s` when on. Interval fixed at 5 s
  (no config — YAGNI). Auto-refresh pauses while in Filter or Preview mode.
- State: `auto_refresh: bool`, `last_refresh: std::time::Instant` on `App`.

## Feature 3 — Image preview (PNG/JPEG)

- `build_preview` detects content by magic bytes:
  - `%PDF` → existing poppler path (unchanged);
  - `\x89PNG` → decode via `image` crate;
  - `\xFF\xD8\xFF` (JPEG) → decode via `image` crate (enable `jpeg` feature);
  - anything else → error `solo hay preview para PDFs o imágenes (PNG/JPEG)`.
- Images are single-page (`total_pages = 1`, ←→ no-ops); rendered by the
  existing `HalfblockImage` widget; no tempdir needed for the image path.
- Unit tests: generated PNG accepted, generated JPEG accepted, garbage
  rejected, PDF fixture still works.

## Feature 4 — Configurable download directory

- Precedence: `--download-dir <path>` flag on `alberto tui` >
  `download_dir` in the active profile > current dir (today's behavior).
- Config: `Profile` gains `pub download_dir: Option<String>`; a leading `~/`
  expands to the home directory. Resolution is a pure, unit-tested function:
  `fn resolve_download_dir(flag: Option<PathBuf>, profile: Option<&str>) -> PathBuf`.
- The TUI `d` key writes `<dir>/<sanitized-name>`, creating the directory
  (`create_dir_all`) if missing, and reports the absolute path (existing
  behavior otherwise unchanged, including filename sanitization).
- `config show` prints the profile's `download_dir` when set.

## Error handling & quality bar

- Same gates as v0.2.0: `cargo fmt --check`, `clippy --all-targets -D
  warnings`, full test suite green (30 existing tests + new unit tests).
- TDD for the pure functions (fuzzy_match, magic detection, download-dir
  resolution). Auto-refresh timing logic kept trivially thin (toggle + elapsed
  check) — verified by types/compile + manual run, not timed tests.
- All user-facing strings Spanish; keys documented in the status hint and in
  `docs/manual-alberto-cli.md` (TUI section) + README key list if present.

## Release

- Bump to 0.3.0, tag `v0.3.0` → existing release pipeline (binaries,
  installer, deb/rpm via dispatch if the release event doesn't chain).
- Retry `cargo publish` and the Homebrew formula job — both currently blocked
  on user tokens (crates.io 403; `HOMEBREW_TAP_TOKEN` unset); if still
  blocked, note and skip.

## Success criteria

- `/` narrows a 100-item folder to matches as you type, with working
  navigation on the filtered view.
- `a` keeps a folder view live without keypresses; toggling off stops fetches.
- `p` previews PNG and JPEG nodes in the terminal exactly like PDFs.
- With `download_dir = "~/Descargas"` in the profile, `d` writes there and
  says so.
