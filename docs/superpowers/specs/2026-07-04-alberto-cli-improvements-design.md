# alberto-cli — Improvement & Distribution Design

**Date:** 2026-07-04
**Status:** Approved by Oscar (design review, this date)

## Context

`alberto-cli` is a Rust CLI (`alberto` binary) for NodeService, freshly extracted
from the `umbrella_nodeservice` repo (`clients/alberto-cli`, `development` branch)
into its own repository at `~/Code/Chile/alberto-cli` with full git history.

Current state (~1,300 lines):

- `upload` — gRPC client-streaming upload with progress bar, retries, idempotency
  (`client_ref` UUID), variants plain/assoc/signed.
- `node *` — 15 subcommands over NodeManagerService (get, ids, by-type, by-path,
  children, user, datamerge, dataupdate, bulk-datamerge, patch, get-in, by-name,
  create, add/remove-secondary).
- `tenant *`, `admin *` — tenant get/create/doclib/home/package; folders, default
  groups, indexes, doclib types.
- `download` — node content via gRPC `NodeContent`.
- `tui` — ratatui two-pane browser with in-terminal PDF preview (poppler
  `pdftoppm` → truecolor half-blocks).
- Auth: `x-api-key` metadata on every RPC; endpoints default to localhost
  port-forwards, override via flags or `ALBERTO_GRPC_ENDPOINT` / `ALBERTO_API_KEY`.
- Structure: `main.rs` (774 lines: clap types + one big match), `tui.rs` (499
  lines). Two tests (PDF preview pipeline). No CI, no config file, no license.

## Goals

1. Host the project on GitHub with CI.
2. Make it installable from the internet: `cargo install`, Homebrew, Linux
   packages, curl-install script.
3. Improve developer UX: config profiles, output modes, completions, errors.
4. Improve code health: modularize, de-duplicate handlers, add tests.
5. Then grow new features on that base.

## Decisions

- **Approach:** phased roadmap (foundation → code health → UX → distribution →
  features). Chosen over distribution-first (would publish unpolished code) and
  feature-first (features multiply the cost of the missing config layer).
- **Repo visibility:** public, under Oscar's personal GitHub account —
  required for crates.io publishing, which the "install from internet via cargo"
  goal implies. Verified: no secrets in the code or history-extracted files; API
  keys come only from env/flags. The `.proto` files and internal RPC names do
  become public. If this becomes a concern, fallback is a private repo with
  Homebrew tap + GitHub Releases only (no crates.io).
- **Crate name:** `alberto-cli` (verified free on crates.io on 2026-07-03; bare
  `alberto` is taken). Binary remains `alberto`.
- **License:** dual MIT OR Apache-2.0 (Rust ecosystem convention).
- **Release tooling:** `cargo-dist` — one tool generates the GitHub Actions
  release pipeline, mac/linux binaries, shell installer, Homebrew formula (pushed
  to a personal `homebrew-tap` repo), and `.deb`/`.rpm` packages.

## Phase 0 — GitHub repo & CI

- Create public GitHub repo `alberto-cli`; push existing history (branch
  `development`, plus a `main` default branch).
- Add `LICENSE-MIT`, `LICENSE-APACHE`, license/`repository` fields in Cargo.toml.
- `.github/workflows/ci.yml`: `cargo fmt --check`, `cargo clippy -- -D warnings`,
  `cargo test` on linux + macos runners.
- The `preview_de_pdf_real` test needs a PDF and poppler: commit a tiny fixture
  PDF at `tests/fixtures/one-page.pdf`, install poppler in CI, and change the
  test to fall back to the fixture when `TEST_PDF` is unset.

## Phase 1 — Code health

- Split `main.rs` into modules with clear boundaries:
  - `cli.rs` — clap Parser/Subcommand types only.
  - `client.rs` — channel construction, `with_key`, `print_monadic`, generated
    proto module includes.
  - `commands/upload.rs`, `commands/node.rs`, `commands/tenant.rs`,
    `commands/admin.rs`, `commands/download.rs` — handlers.
  - `tui.rs` stays.
- De-duplicate the ~28 identical handler bodies (`build request → nm_client →
  print_monadic`) behind one generic helper.
- Targeted fixes found in review:
  - TUI opens a new gRPC channel per fetch — hold one client/channel in `App`.
  - TUI `r` (refresh) is a stub — make it re-fetch the current level.
  - TUI `d` (download) writes the node name into CWD unsanitized — sanitize the
    filename (strip path separators) and report the absolute path written.
- Testing: add integration tests against an in-process mock NodeManagerService /
  BinaryTransferService (tonic server on an ephemeral port): one happy path and
  one error path per command family (node, tenant, admin, upload, download).

## Phase 2 — Developer UX

- **Config profiles:** `~/.config/alberto/config.toml`:

  ```toml
  default_profile = "local"

  [profiles.local]
  endpoint = "http://127.0.0.1:9090"
  api_key = "..."

  [profiles.qa]
  endpoint = "http://qa-host:9090"
  api_key = "..."
  ```

  Resolution order: explicit flags > env vars > `--profile`/`ALBERTO_PROFILE` >
  `default_profile`. `alberto config init|list|show` manage the file. `api_key`
  becomes optional at the flag level once a profile can supply it; a missing key
  after full resolution is a friendly error naming all three sources.
- **Output modes:** `--output pretty|json|raw|table` (default `pretty`, current
  behavior). `json` = compact single-line for piping; `table` = column view for
  list-shaped results (children, by-type, ids).
- **Completions:** `alberto completions <shell>` via `clap_complete`.
- **Errors:** map common failures (connection refused, unauthenticated, deadline
  exceeded) to short hints, e.g. "¿está corriendo el port-forward?".

## Phase 3 — Distribution

- Adopt `cargo-dist`: tag `vX.Y.Z` → GitHub Actions builds
  aarch64/x86_64-apple-darwin and x86_64/aarch64-unknown-linux-gnu (+musl)
  binaries, generates the shell installer, updates the Homebrew formula in a
  personal `homebrew-tap` repo, and produces `.deb`/`.rpm`.
- Publish to crates.io as `alberto-cli`; `cargo install alberto-cli` installs the
  `alberto` binary. Note: `cargo install` builds from the crate — the `proto/`
  dir and `build.rs` ship in the crate package (protoc needed at build time; use
  `protoc-bin-vendored` or commit generated code if this proves painful — decide
  at implementation time, prefer `protoc-bin-vendored`).
- README: English install/quick-start section up top, existing Spanish manual
  stays in `docs/manual-alberto-cli.md` and is linked.

## Phase 4 — New features (prioritized backlog, each its own mini-design)

1. TUI fuzzy search/filter within the current level (`/` to filter).
2. TUI real refresh (from Phase 1) extended: periodic auto-refresh toggle.
3. TUI upload: pick a local file, upload into the current folder.
4. TUI image preview (PNG/JPEG) reusing the half-block renderer.
5. Configurable download directory (config file + `d` prompt).
6. Bulk upload from a folder or CSV manifest (`alberto upload-batch`).
7. `--watch` mode for `node children`.
8. `node search` — blocked on the backend exposing a search RPC.

## Error handling & quality bar (all phases)

- `clippy -D warnings` clean; `rustfmt` enforced by CI.
- Every command returns non-zero exit and stderr `{:error, ...}` on business
  errors (current behavior, preserved and tested).
- New modules get unit tests; user-facing behavior changes get integration tests.

## Out of scope

- Removing `clients/alberto-cli` from the umbrella repo's `development` branch
  (separate decision, touches the shared Bitbucket remote).
- Backend (NodeService) changes — e.g. the search RPC for feature 8.
- Windows support (revisit on demand; nothing known to block it except poppler).

## Success criteria

- A teammate on a clean mac/linux machine can install with one command
  (brew/cargo/curl) and run `alberto --version`.
- CI green on every PR; releases produced by tagging.
- `main.rs` under ~150 lines; no handler body repeated.
- Config profiles remove per-command endpoint/key flags in daily use.
