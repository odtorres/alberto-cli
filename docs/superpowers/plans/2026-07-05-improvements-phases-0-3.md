# alberto-cli Improvements (Phases 0–3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Take the freshly-extracted alberto-cli repo to a public GitHub project with CI, clean module structure, config profiles, output modes, and one-command installation (cargo/homebrew/curl/deb/rpm).

**Architecture:** Split the 774-line `main.rs` into `cli.rs` (clap types), `client.rs` (gRPC plumbing), and `commands/*` handlers behind a `src/lib.rs` so integration tests can reuse the generated proto types to build an in-process mock gRPC server. Config profiles resolve endpoint/api-key with precedence flags > env > profile > defaults. Distribution is cargo-dist (binaries, shell installer, Homebrew tap) plus an nfpm workflow for deb/rpm, plus crates.io.

**Tech Stack:** Rust 2021, tonic 0.12 / prost 0.13, clap 4 (derive+env), ratatui 0.29, tokio 1. New deps: serde, toml, dirs, clap_complete, protoc-bin-vendored (build), assert_cmd + predicates + tokio-stream (dev).

**Spec:** `docs/superpowers/specs/2026-07-04-alberto-cli-improvements-design.md`

## Global Constraints

- Crate name `alberto-cli`, binary name `alberto` (crates.io name verified free 2026-07-03).
- License: `MIT OR Apache-2.0`.
- CI gates (every task must leave these green): `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
- Business errors keep the contract: stderr message contains `{:error, <reason>}`, non-zero exit.
- All user-facing CLI strings stay Spanish (match existing style); README gains an English section.
- Working directory for all commands: `/Users/odtorres/Code/Chile/alberto-cli`. Branch: `development`.
- `GH_USER` below means the authenticated GitHub username — obtain once with `gh api user -q .login` and substitute literally wherever `GH_USER` appears.
- Commit messages end with `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.

---

## Phase 0 — Lint baseline, tests, CI, GitHub

### Task 1: Fix clippy/fmt baseline

**Files:**
- Modify: `src/main.rs:1-10` (doc comment), `src/main.rs:22-24` (transfer module)

**Interfaces:**
- Produces: a repo where `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` pass — every later task relies on this gate.

- [ ] **Step 1: Reproduce the two clippy errors**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: FAIL with `doc list item overindented` at `src/main.rs:4` and `large size difference between variants` in generated `transfer.rs`.

- [ ] **Step 2: Fix the doc list indent**

In `src/main.rs` replace lines 3–4:

```rust
//! * `upload`   → gRPC client-streaming (feature upload_by_streaming_with_backpressure)
//!                con idempotencia (client_ref) y reintentos automáticos.
```

with:

```rust
//! * `upload`   → gRPC client-streaming (feature upload_by_streaming_with_backpressure)
//!   con idempotencia (client_ref) y reintentos automáticos.
```

- [ ] **Step 3: Allow large_enum_variant in the generated transfer module**

In `src/main.rs` replace:

```rust
pub mod transfer {
    tonic::include_proto!("transfer");
}
```

with:

```rust
pub mod transfer {
    #![allow(clippy::large_enum_variant)]
    tonic::include_proto!("transfer");
}
```

(The lint fires on the prost-generated `chunk_request::Payload` enum; boxing it would fight the generated API.)

- [ ] **Step 4: Apply rustfmt**

Run: `cargo fmt`
(Known diffs: multi-line `#[arg(...)]` attributes. Accept all.)

- [ ] **Step 5: Verify gates**

Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --check`
Expected: both exit 0.

- [ ] **Step 6: Commit**

```bash
git add -A src
git commit -m "chore: clippy -D warnings + rustfmt clean baseline"
```

### Task 2: PDF test fixture with TEST_PDF fallback

**Files:**
- Create: `tests/fixtures/one-page.pdf`
- Modify: `src/tui.rs:486-493` (test `preview_de_pdf_real`)

**Interfaces:**
- Produces: `cargo test` passes with no env vars (poppler still required on the machine). CI (Task 4) depends on this.

- [ ] **Step 1: Run the suite to see the current failure**

Run: `cargo test`
Expected: FAIL — `preview_de_pdf_real` panics with `export TEST_PDF=/ruta.pdf: NotPresent`.

- [ ] **Step 2: Create the fixture (minimal single-page PDF, renders with pdftoppm)**

```bash
mkdir -p tests/fixtures
printf '%%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]>>endobj\nxref\n0 4\n0000000000 65535 f \ntrailer<</Size 4/Root 1 0 R>>\nstartxref\n0\n%%%%EOF\n' > tests/fixtures/one-page.pdf
```

- [ ] **Step 3: Make the test fall back to the fixture**

In `src/tui.rs`, replace the first line of `preview_de_pdf_real`:

```rust
let pdf = std::env::var("TEST_PDF").expect("export TEST_PDF=/ruta.pdf");
```

with:

```rust
let pdf = std::env::var("TEST_PDF").unwrap_or_else(|_| {
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/one-page.pdf").to_string()
});
```

- [ ] **Step 4: Verify**

Run: `cargo test`
Expected: `test result: ok. 2 passed` (verified 2026-07-03 that this exact fixture renders >100px and pdfinfo reports 1 page).

- [ ] **Step 5: Commit**

```bash
git add tests/fixtures/one-page.pdf src/tui.rs
git commit -m "test: fixture PDF con fallback para preview_de_pdf_real"
```

### Task 3: Vendored protoc in build.rs

**Files:**
- Modify: `Cargo.toml` (build-dependencies), `build.rs`

**Interfaces:**
- Produces: builds succeed on machines/CI without protoc installed; `cargo install alberto-cli` works from the crate package (spec Phase 3 requirement, front-loaded so CI never installs protoc).

- [ ] **Step 1: Add the build-dependency**

In `Cargo.toml` under `[build-dependencies]`:

```toml
[build-dependencies]
tonic-build = "0.12"
protoc-bin-vendored = "3"
```

- [ ] **Step 2: Point PROTOC at the vendored binary**

Replace `build.rs` entirely with:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // protoc vendorizado: la compilación no depende de protoc del sistema
    // (necesario para `cargo install` y para CI limpia).
    std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?);

    // Espejos de apps/nodeservice/priv/protos/*.proto (repo umbrella).
    // Si el contrato cambia en el servidor, copiar aquí los .proto actualizados.
    tonic_build::compile_protos("proto/binary_transfer.proto")?;
    tonic_build::compile_protos("proto/node_manager.proto")?;
    Ok(())
}
```

- [ ] **Step 3: Verify a from-scratch build uses it**

Run: `cargo clean && cargo build && cargo test`
Expected: builds and 2 tests pass (first build recompiles everything, ~1-2 min).

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock build.rs
git commit -m "build: protoc vendorizado (protoc-bin-vendored)"
```

### Task 4: CI workflow

**Files:**
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- Produces: CI running fmt/clippy/test on ubuntu + macos for pushes to `main`/`development` and PRs.

- [ ] **Step 1: Write the workflow**

```yaml
name: CI

on:
  push:
    branches: [main, development]
  pull_request:

env:
  CARGO_TERM_COLOR: always

jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --check
      - run: cargo clippy --all-targets -- -D warnings

  test:
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Install poppler (linux)
        if: runner.os == 'Linux'
        run: sudo apt-get update && sudo apt-get install -y poppler-utils
      - name: Install poppler (macos)
        if: runner.os == 'macOS'
        run: brew install poppler
      - run: cargo test
```

- [ ] **Step 2: Sanity-check the gates locally (same commands CI runs)**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
Expected: all pass.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: fmt + clippy + test en ubuntu y macos"
```

(CI itself is verified after Task 6 pushes the repo.)

### Task 5: Licenses + Cargo metadata

**Files:**
- Create: `LICENSE-MIT`, `LICENSE-APACHE`
- Modify: `Cargo.toml` `[package]`

- [ ] **Step 1: LICENSE-MIT**

```text
MIT License

Copyright (c) 2026 Oscar Daniel Torres Hernández

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

- [ ] **Step 2: LICENSE-APACHE**

```bash
curl -fsSL https://www.apache.org/licenses/LICENSE-2.0.txt -o LICENSE-APACHE
```

- [ ] **Step 3: Cargo.toml metadata**

Determine the username: `GH_USER=$(gh api user -q .login)`. Then extend `[package]`:

```toml
[package]
name = "alberto-cli"
version = "0.1.0"
edition = "2021"
description = "CLI para NodeService: upload por gRPC streaming (backpressure), operaciones de nodos y TUI con preview de PDFs"
license = "MIT OR Apache-2.0"
repository = "https://github.com/GH_USER/alberto-cli"
readme = "README.md"
keywords = ["cli", "grpc", "tui", "document-management"]
categories = ["command-line-utilities"]
include = ["src/**", "proto/**", "build.rs", "README.md", "LICENSE-*"]
```

(`include` keeps the crates.io package slim — docs/tests/fixtures stay out; `cargo publish` only verifies the build, which doesn't need them.)

- [ ] **Step 4: Verify + commit**

Run: `cargo check`
Expected: OK (warning about no README is acceptable until Task 20).

```bash
git add LICENSE-MIT LICENSE-APACHE Cargo.toml
git commit -m "chore: licencias MIT/Apache-2.0 + metadata de Cargo"
```

### Task 6: Create GitHub repo and push

**Files:** none (repo operations)

- [ ] **Step 1: Check gh auth**

Run: `gh auth status`
If not authenticated: ask the user to run `! gh auth login` (interactive; user action).

- [ ] **Step 2: Create branch main at current HEAD**

```bash
git branch main development
```

- [ ] **Step 3: Create the public repo and push both branches**

```bash
gh repo create alberto-cli --public --source=. --remote=origin \
  --description "CLI para NodeService: gRPC streaming upload, node ops y TUI con preview de PDFs"
git push -u origin main development
gh repo edit --default-branch main
```

- [ ] **Step 4: Verify CI is green**

Run: `gh run watch $(gh run list --limit 1 --json databaseId -q '.[0].databaseId')`
Expected: CI completes with success on both jobs. If a job fails, read the log (`gh run view --log-failed`), fix, commit, push, re-verify.

---

## Phase 1 — Code health

### Task 7: Module split — lib.rs, cli.rs, client.rs

**Files:**
- Create: `src/lib.rs`, `src/cli.rs`, `src/client.rs`
- Modify: `src/main.rs`, `src/tui.rs` (imports only)

**Interfaces:**
- Produces (used by every later task):
  - `alberto_cli::cli::{Cli, Cmd, NodeCmd, TenantCmd, AdminCmd, UploadArgs, GrpcOpts}` — all clap types, all fields `pub`.
  - `alberto_cli::client::{transfer, nodemanager}` — generated proto modules (clients AND servers).
  - `pub async fn client::nm_client(grpc: &GrpcOpts) -> Result<NodeManagerServiceClient<Channel>>`
  - `pub fn client::with_key<T>(req: T, api_key: &str) -> Result<tonic::Request<T>>` — **note: takes `&str`, not `&GrpcOpts`**.
  - `pub fn client::print_monadic(reply: nodemanager::MonadicReply) -> Result<()>`

- [ ] **Step 1: Create `src/cli.rs`**

Move from `src/main.rs`, verbatim except making every struct, enum, and field `pub`:
- `struct Cli` (lines 36-45), `enum Cmd` (47-122), `struct GrpcOpts` (124-132), `enum NodeCmd` (134-263), `enum TenantCmd` (265-311), `enum AdminCmd` (313-370).

Two mechanical changes while moving:
1. File header: `//! Definición de la CLI (clap). Sin lógica: solo tipos.` and `use clap::{Parser, Subcommand}; use std::path::PathBuf;`
2. Convert the inline `Cmd::Upload { ...15 fields... }` variant to `Upload(UploadArgs)` with:

```rust
/// Argumentos de `alberto upload` (variantes plain/assoc/signed).
#[derive(clap::Args)]
pub struct UploadArgs {
    /// Archivo a subir
    pub file: PathBuf,
    /// Tipo documental del nodo (ej: factura)
    #[arg(long = "type")]
    pub node_type: String,
    /// Título del nodo (default: nombre del archivo)
    #[arg(long)]
    pub title: Option<String>,
    /// Descripción
    #[arg(long, default_value = "")]
    pub description: String,
    /// unique_id del nodo padre (debe existir)
    #[arg(long)]
    pub parent: String,
    /// Username que sube (debe existir)
    #[arg(long)]
    pub user: String,
    /// Tenant (informativo; el efectivo se hereda del parent)
    #[arg(long, default_value = "")]
    pub tenant: String,
    /// Metadata JSON del nodo, ej: '{"rut":"1-9"}'
    #[arg(long, default_value = "{}")]
    pub data: String,
    /// unique_id a asociar como secondary_parent (activa variante assoc)
    #[arg(long)]
    pub assoc: Option<String>,
    /// unique_id del contenido firmado a referenciar (activa variante signed)
    #[arg(long)]
    pub signed_ref: Option<String>,
    /// Endpoint gRPC
    #[arg(long, env = "ALBERTO_GRPC_ENDPOINT", default_value = "http://127.0.0.1:9090")]
    pub endpoint: String,
    /// API key (header x-api-key, igual que la capa HTTP)
    #[arg(long, env = "ALBERTO_API_KEY")]
    pub api_key: String,
    /// Intentos totales ante fallas de red/timeout (la idempotencia evita duplicados)
    #[arg(long, default_value_t = 3)]
    pub retries: u32,
}
```

- [ ] **Step 2: Create `src/client.rs`**

```rust
//! Conexión gRPC, autenticación (x-api-key) y presentación de respuestas.

use std::time::Duration;

use anyhow::{bail, Context, Result};

use crate::cli::GrpcOpts;

pub mod transfer {
    #![allow(clippy::large_enum_variant)]
    tonic::include_proto!("transfer");
}

pub mod nodemanager {
    tonic::include_proto!("nodemanager");
}

use nodemanager::node_manager_service_client::NodeManagerServiceClient;

pub async fn nm_client(
    grpc: &GrpcOpts,
) -> Result<NodeManagerServiceClient<tonic::transport::Channel>> {
    let channel = tonic::transport::Channel::from_shared(grpc.endpoint.clone())
        .context("endpoint invalido (usa http://host:puerto)")?
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .connect()
        .await
        .context("no se pudo conectar al endpoint gRPC")?;

    // NodeContent devuelve el binario completo en un mensaje: subir el límite
    // de decode (default tonic: 4 MB) para archivos grandes.
    Ok(NodeManagerServiceClient::new(channel).max_decoding_message_size(1024 * 1024 * 1024))
}

pub fn with_key<T>(req: T, api_key: &str) -> Result<tonic::Request<T>> {
    let mut request = tonic::Request::new(req);
    request
        .metadata_mut()
        .insert("x-api-key", api_key.parse().context("api key con caracteres invalidos")?);
    Ok(request)
}

/// Imprime la respuesta monádica: ok=true -> result_json a stdout;
/// ok=false -> error a stderr y exit != 0 ({:error, _}).
pub fn print_monadic(reply: nodemanager::MonadicReply) -> Result<()> {
    if reply.ok {
        match serde_json::from_str::<serde_json::Value>(&reply.result_json) {
            Ok(v) => println!("{}", serde_json::to_string_pretty(&v)?),
            Err(_) => println!("{}", reply.result_json),
        }
        Ok(())
    } else {
        bail!("{{:error, {}}}", reply.error);
    }
}
```

- [ ] **Step 3: Create `src/lib.rs`**

```rust
//! alberto — CLI para NodeService (lib para reuso en tests de integración).

pub mod cli;
pub mod client;
pub mod tui;
```

- [ ] **Step 4: Rewire `src/main.rs` and `src/tui.rs`**

`src/main.rs`: delete everything moved in steps 1–2 (module declarations for `transfer`/`nodemanager`, clap types, `nm_client`, `with_key`, `print_monadic`, `mod tui`). At the top:

```rust
use alberto_cli::cli::{self, Cli};
use alberto_cli::client::{self, nodemanager, transfer};
use alberto_cli::tui;
```

The big `match cli.cmd` stays in `main.rs` for now (Task 8 moves it out). Adjust:
- `Cmd::Upload { file, node_type, ... }` arm becomes `cli::Cmd::Upload(a)` using `a.file`, `a.node_type`, etc.
- Every `with_key(req, &grpc)` becomes `client::with_key(req, &grpc.api_key)`.
- Keep `upload_with_retries` / `upload_once` in `main.rs` (they move in Task 8).

`src/tui.rs`: replace `use crate::{nm_client, nodemanager, with_key, GrpcOpts};` with:

```rust
use crate::cli::GrpcOpts;
use crate::client::{nm_client, nodemanager, with_key};
```

and update its three `with_key(req, grpc)` calls to `with_key(req, &grpc.api_key)`.

- [ ] **Step 5: Verify gates**

Run: `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`
Expected: pass, 2 tests. `alberto --help` output unchanged: `cargo run -q -- --help`.

- [ ] **Step 6: Commit**

```bash
git add -A src
git commit -m "refactor: split en lib + cli.rs + client.rs (sin cambios de comportamiento)"
```

### Task 8: commands/* modules + nm_call dedupe helper

**Files:**
- Create: `src/commands/mod.rs`, `src/commands/upload.rs`, `src/commands/node.rs`, `src/commands/tenant.rs`, `src/commands/admin.rs`, `src/commands/download.rs`
- Modify: `src/lib.rs`, `src/main.rs`, `src/client.rs`

**Interfaces:**
- Consumes: Task 7's `cli`/`client` modules.
- Produces:
  - `pub async fn client::nm_call<T, F, Fut>(grpc: &GrpcOpts, req: T, call: F) -> Result<()>` — the one helper all 28 monadic handlers go through.
  - `commands::{upload,node,tenant,admin,download}::run(...)` — exact signatures in Step 2.
  - `src/main.rs` under 150 lines (spec success criterion).

- [ ] **Step 1: Add `nm_call` to `src/client.rs`**

```rust
/// Ejecuta una RPC monádica: conecta, autentica, llama e imprime.
/// Colapsa el patrón repetido en los ~28 handlers.
pub async fn nm_call<T, F, Fut>(grpc: &GrpcOpts, req: T, call: F) -> Result<()>
where
    F: FnOnce(
        NodeManagerServiceClient<tonic::transport::Channel>,
        tonic::Request<T>,
    ) -> Fut,
    Fut: std::future::Future<
        Output = std::result::Result<tonic::Response<nodemanager::MonadicReply>, tonic::Status>,
    >,
{
    let client = nm_client(grpc).await?;
    let reply = call(client, with_key(req, &grpc.api_key)?).await?.into_inner();
    print_monadic(reply)
}
```

- [ ] **Step 2: Create the command modules**

`src/commands/mod.rs`:

```rust
pub mod admin;
pub mod download;
pub mod node;
pub mod tenant;
pub mod upload;

use anyhow::{Context, Result};

/// Valida que un flag sea JSON antes de mandarlo al servidor.
pub(crate) fn valid_json(s: &str, flag: &str) -> Result<()> {
    serde_json::from_str::<serde_json::Value>(s)
        .map(|_| ())
        .with_context(|| format!("{flag} no es JSON valido"))
}
```

`src/commands/node.rs` (all 15 arms, complete):

```rust
//! `alberto node *` — operaciones de nodos vía NodeManagerService.

use anyhow::Result;

use super::valid_json;
use crate::cli::NodeCmd;
use crate::client::{nm_call, nodemanager as nm};

pub async fn run(cmd: NodeCmd) -> Result<()> {
    match cmd {
        NodeCmd::Get { id, grpc } => {
            nm_call(&grpc, nm::UniqueIdRequest { unique_id: id }, |mut c, r| async move {
                c.node_get(r).await
            })
            .await
        }
        NodeCmd::Ids { ids, node_type, grpc } => {
            nm_call(&grpc, nm::IdsRequest { ids, r#type: node_type }, |mut c, r| async move {
                c.ids(r).await
            })
            .await
        }
        NodeCmd::ByType { node_type, tenant, grpc } => {
            nm_call(&grpc, nm::HomeRequest { tenant, r#type: node_type }, |mut c, r| async move {
                c.by_type(r).await
            })
            .await
        }
        NodeCmd::ByPath { path, tenant, grpc } => {
            nm_call(&grpc, nm::ByPathRequest { tenant, path }, |mut c, r| async move {
                c.by_path(r).await
            })
            .await
        }
        NodeCmd::Children { id, secondary, grpc } => {
            nm_call(&grpc, nm::NodeChildRequest { unique_id: id, secondary }, |mut c, r| async move {
                c.node_child(r).await
            })
            .await
        }
        NodeCmd::User { username, grpc } => {
            nm_call(
                &grpc,
                nm::UserRequest { username, password: String::new() },
                |mut c, r| async move { c.user(r).await },
            )
            .await
        }
        NodeCmd::Datamerge { id, data, grpc } => {
            valid_json(&data, "--data")?;
            nm_call(
                &grpc,
                nm::DatamergeRequest { unique_id: id, data_json: data },
                |mut c, r| async move { c.datamerge(r).await },
            )
            .await
        }
        NodeCmd::DataUpdate { id, data, grpc } => {
            valid_json(&data, "--data")?;
            nm_call(
                &grpc,
                nm::DataUpdateRequest { unique_id: id, data_json: data },
                |mut c, r| async move { c.data_update(r).await },
            )
            .await
        }
        NodeCmd::BulkDatamerge { changes, grpc } => {
            valid_json(&changes, "--changes")?;
            nm_call(
                &grpc,
                nm::BulkDatamergeRequest { changes_json: changes },
                |mut c, r| async move { c.bulk_datamerge(r).await },
            )
            .await
        }
        NodeCmd::Patch { envelope_path, path, data, grpc } => {
            valid_json(&data, "--data")?;
            nm_call(
                &grpc,
                nm::PatchRequest { envelope_path, path, data_json: data },
                |mut c, r| async move { c.patch(r).await },
            )
            .await
        }
        NodeCmd::GetIn { node_path, path, grpc } => {
            nm_call(&grpc, nm::GetRequest { node_path, path }, |mut c, r| async move {
                c.get(r).await
            })
            .await
        }
        NodeCmd::ByName { name, grpc } => {
            nm_call(&grpc, nm::NameRequest { name }, |mut c, r| async move {
                c.node_by_name(r).await
            })
            .await
        }
        NodeCmd::Create { parent, node_type, data, grpc } => {
            valid_json(&data, "--data")?;
            nm_call(
                &grpc,
                nm::NodeCreateRequest { parent_id: parent, data_json: data, r#type: node_type },
                |mut c, r| async move { c.node_create(r).await },
            )
            .await
        }
        NodeCmd::AddSecondary { child_id, parent_id, grpc } => {
            nm_call(
                &grpc,
                nm::SecondaryParentRequest { child_id, parent_id },
                |mut c, r| async move { c.add_secondary_parent(r).await },
            )
            .await
        }
        NodeCmd::RemoveSecondary { child_id, parent_id, grpc } => {
            nm_call(
                &grpc,
                nm::SecondaryParentRequest { child_id, parent_id },
                |mut c, r| async move { c.remove_secondary_parent(r).await },
            )
            .await
        }
    }
}
```

`src/commands/tenant.rs` (5 arms, same pattern):

```rust
//! `alberto tenant *` — operaciones de tenant.

use anyhow::Result;

use crate::cli::TenantCmd;
use crate::client::{nm_call, nodemanager as nm};

pub async fn run(cmd: TenantCmd) -> Result<()> {
    match cmd {
        TenantCmd::Get { tenant, grpc } => {
            nm_call(&grpc, nm::TenantGetRequest { tenant }, |mut c, r| async move {
                c.tenant_get(r).await
            })
            .await
        }
        TenantCmd::Create { tenant, title, description, dni, company, email, grpc } => {
            nm_call(
                &grpc,
                nm::TenantCreateRequest {
                    tenant,
                    title,
                    description,
                    dni,
                    company_name: company,
                    email,
                },
                |mut c, r| async move { c.tenant_create(r).await },
            )
            .await
        }
        TenantCmd::Doclib { tenant, grpc } => {
            nm_call(&grpc, nm::TenantRequest { tenant }, |mut c, r| async move {
                c.doc_lib(r).await
            })
            .await
        }
        TenantCmd::Home { tenant, node_type, grpc } => {
            nm_call(&grpc, nm::HomeRequest { tenant, r#type: node_type }, |mut c, r| async move {
                c.home(r).await
            })
            .await
        }
        TenantCmd::Package { tenant, node_type, grpc } => {
            nm_call(&grpc, nm::PackageRequest { tenant, r#type: node_type }, |mut c, r| async move {
                c.package(r).await
            })
            .await
        }
    }
}
```

`src/commands/admin.rs` (7 arms):

```rust
//! `alberto admin *` — folders, grupos default, índices.

use anyhow::Result;

use super::valid_json;
use crate::cli::AdminCmd;
use crate::client::{nm_call, nodemanager as nm};

pub async fn run(cmd: AdminCmd) -> Result<()> {
    match cmd {
        AdminCmd::Folder { parent, name, title, description, data, grpc } => {
            valid_json(&data, "--data")?;
            nm_call(
                &grpc,
                nm::FolderRequest { parent_id: parent, data_json: data, name, title, description },
                |mut c, r| async move { c.folder(r).await },
            )
            .await
        }
        AdminCmd::DefaultGroup { name, parent, grpc } => {
            nm_call(&grpc, nm::DefaultGroupRequest { name, parent_id: parent }, |mut c, r| async move {
                c.default_group(r).await
            })
            .await
        }
        AdminCmd::ColaboratorGroup { parent, grpc } => {
            nm_call(&grpc, nm::ParentRequest { parent_id: parent }, |mut c, r| async move {
                c.default_colaborator_group(r).await
            })
            .await
        }
        AdminCmd::ConsumerGroup { parent, grpc } => {
            nm_call(&grpc, nm::ParentRequest { parent_id: parent }, |mut c, r| async move {
                c.default_consumer_group(r).await
            })
            .await
        }
        AdminCmd::AdministratorGroup { parent, grpc } => {
            nm_call(&grpc, nm::ParentRequest { parent_id: parent }, |mut c, r| async move {
                c.default_administrator_group(r).await
            })
            .await
        }
        AdminCmd::Indexs { grpc } => {
            nm_call(&grpc, nm::EmptyRequest {}, |mut c, r| async move { c.indexs(r).await }).await
        }
        AdminCmd::DoclibTypes { grpc } => {
            nm_call(&grpc, nm::EmptyRequest {}, |mut c, r| async move {
                c.doc_libs_types(r).await
            })
            .await
        }
    }
}
```

`src/commands/download.rs`:

```rust
//! `alberto download` — contenido binario de un nodo (NodeContent).

use std::path::PathBuf;

use anyhow::{bail, Result};

use crate::cli::GrpcOpts;
use crate::client::{nm_client, nodemanager as nm, with_key};

pub async fn run(id: String, dest: Option<PathBuf>, grpc: GrpcOpts) -> Result<()> {
    let out = dest.unwrap_or_else(|| PathBuf::from(format!("{id}.bin")));
    let req = nm::UniqueIdRequest { unique_id: id };
    let mut c = nm_client(&grpc).await?;
    let reply = c.node_content(with_key(req, &grpc.api_key)?).await?.into_inner();
    if reply.ok {
        tokio::fs::write(&out, &reply.content).await?;
        eprintln!("descargado: {} ({} bytes)", out.display(), reply.content.len());
        Ok(())
    } else {
        bail!("{{:error, {}}}", reply.error);
    }
}
```

`src/commands/upload.rs`: move `upload_with_retries`, `upload_once`, and `const CHUNK_SIZE` from `main.rs` verbatim, plus the argument-handling block from the old `Cmd::Upload` arm as:

```rust
//! `alberto upload` — gRPC client-streaming con idempotencia y reintentos.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use tokio::io::AsyncReadExt;

use super::valid_json;
use crate::cli::UploadArgs;
use crate::client::transfer::binary_transfer_service_client::BinaryTransferServiceClient;
use crate::client::transfer::{chunk_request::Payload, ChunkRequest, TransferMeta};

const CHUNK_SIZE: usize = 64 * 1024;

pub async fn run(a: UploadArgs) -> Result<()> {
    if a.assoc.is_some() && a.signed_ref.is_some() {
        bail!("--assoc y --signed-ref son mutuamente excluyentes");
    }
    valid_json(&a.data, "--data")?;

    let filename = a
        .file
        .file_name()
        .context("ruta de archivo invalida")?
        .to_string_lossy()
        .to_string();

    let variant = if a.assoc.is_some() {
        "assoc"
    } else if a.signed_ref.is_some() {
        "signed"
    } else {
        "plain"
    };

    let meta = TransferMeta {
        tenant: a.tenant,
        r#type: a.node_type,
        title: a.title.unwrap_or_else(|| filename.clone()),
        description: a.description,
        filename,
        parent_id: a.parent,
        username: a.user,
        data_json: a.data,
        variant: variant.into(),
        assoc_id: a.assoc.unwrap_or_default(),
        ref_signed_id: a.signed_ref.unwrap_or_default(),
        // Idempotencia: UNA clave por invocación, compartida por todos
        // los reintentos — un retry jamás duplica el documento.
        client_ref: uuid::Uuid::new_v4().to_string(),
    };

    upload_with_retries(&a.endpoint, &a.file, meta, &a.api_key, a.retries).await
}

// upload_with_retries y upload_once: mover VERBATIM de src/main.rs
// (líneas 633-731 del estado previo a Task 7), cambiando su visibilidad a
// privada del módulo (sin `pub`).
```

- [ ] **Step 3: Shrink `src/main.rs`**

```rust
//! alberto — binario. Toda la lógica vive en la lib (cli/client/commands/tui).

use clap::Parser;

use alberto_cli::cli::{Cli, Cmd};
use alberto_cli::{commands, tui};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Cli::parse().cmd {
        Cmd::Upload(args) => commands::upload::run(args).await,
        Cmd::Node { cmd } => commands::node::run(cmd).await,
        Cmd::Tenant { cmd } => commands::tenant::run(cmd).await,
        Cmd::Admin { cmd } => commands::admin::run(cmd).await,
        Cmd::Tui { tenant, grpc } => tui::run(tenant, grpc),
        Cmd::Download { id, dest, grpc } => commands::download::run(id, dest, grpc).await,
    }
}
```

Add `pub mod commands;` to `src/lib.rs`.

- [ ] **Step 4: Verify gates + line count**

Run: `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test && wc -l src/main.rs`
Expected: pass; `src/main.rs` well under 150 lines. `cargo run -q -- node --help` output identical to before.

- [ ] **Step 5: Commit**

```bash
git add -A src
git commit -m "refactor: commands/* + helper nm_call (28 handlers deduplicados)"
```

### Task 9: Integration tests against an in-process mock gRPC server

**Files:**
- Create: `tests/cli.rs`
- Modify: `Cargo.toml` (dev-dependencies)

**Interfaces:**
- Consumes: `alberto_cli::client::{nodemanager, transfer}` generated **server** traits (tonic-build generates them by default).
- Produces: `tests/cli.rs::spawn_mock() -> SocketAddr` — later tasks (13, 15, 16, 17) add tests to this file.

- [ ] **Step 1: Add dev-dependencies**

```toml
[dev-dependencies]
assert_cmd = "2"
predicates = "3"
tokio-stream = { version = "0.1", features = ["net"] }
```

- [ ] **Step 2: Write the mock + tests (complete file)**

`tests/cli.rs`:

```rust
//! End-to-end: el binario `alberto` contra un NodeService gRPC de mentira.

use std::net::SocketAddr;

use assert_cmd::Command;
use predicates::str::contains;
use tokio::net::TcpListener;
use tonic::{Request, Response, Status};

use alberto_cli::client::nodemanager::node_manager_service_server::{
    NodeManagerService, NodeManagerServiceServer,
};
use alberto_cli::client::nodemanager::*;
use alberto_cli::client::transfer::binary_transfer_service_server::{
    BinaryTransferService, BinaryTransferServiceServer,
};
use alberto_cli::client::transfer::{chunk_request, ChunkRequest, TransferReply};

const KEY: &str = "test-key";

fn auth<T>(req: &Request<T>) -> Result<(), Status> {
    match req.metadata().get("x-api-key") {
        Some(v) if v == KEY => Ok(()),
        _ => Err(Status::unauthenticated("x-api-key invalida")),
    }
}

fn ok_reply(json: &str) -> Result<Response<MonadicReply>, Status> {
    Ok(Response::new(MonadicReply {
        ok: true,
        result_json: json.into(),
        error: String::new(),
        content: vec![],
    }))
}

#[derive(Default)]
struct MockNm;

macro_rules! stub {
    ($($name:ident($req:ty)),* $(,)?) => {
        $(async fn $name(&self, _r: Request<$req>) -> Result<Response<MonadicReply>, Status> {
            Err(Status::unimplemented(stringify!($name)))
        })*
    };
}

#[tonic::async_trait]
impl NodeManagerService for MockNm {
    async fn node_get(
        &self,
        r: Request<UniqueIdRequest>,
    ) -> Result<Response<MonadicReply>, Status> {
        auth(&r)?;
        let id = r.into_inner().unique_id;
        ok_reply(&format!(r#"{{"unique_id":"{id}","name":"doc.pdf","content":true}}"#))
    }

    async fn tenant_get(
        &self,
        r: Request<TenantGetRequest>,
    ) -> Result<Response<MonadicReply>, Status> {
        auth(&r)?;
        let t = r.into_inner().tenant;
        ok_reply(&format!(r#"{{"tenant":"{t}"}}"#))
    }

    async fn indexs(&self, r: Request<EmptyRequest>) -> Result<Response<MonadicReply>, Status> {
        auth(&r)?;
        ok_reply(r#"["rut","folio"]"#)
    }

    // error de negocio: {:error, not_found}
    async fn user(&self, r: Request<UserRequest>) -> Result<Response<MonadicReply>, Status> {
        auth(&r)?;
        Ok(Response::new(MonadicReply {
            ok: false,
            result_json: String::new(),
            error: "not_found".into(),
            content: vec![],
        }))
    }

    async fn node_content(
        &self,
        r: Request<UniqueIdRequest>,
    ) -> Result<Response<MonadicReply>, Status> {
        auth(&r)?;
        Ok(Response::new(MonadicReply {
            ok: true,
            result_json: String::new(),
            error: String::new(),
            content: b"%PDF-fake-content".to_vec(),
        }))
    }

    stub!(
        ids(IdsRequest),
        data_update(DataUpdateRequest),
        bulk_datamerge(BulkDatamergeRequest),
        patch(PatchRequest),
        get(GetRequest),
        datamerge(DatamergeRequest),
        node_by_name(NameRequest),
        package(PackageRequest),
        node_create(NodeCreateRequest),
        tenant_create(TenantCreateRequest),
        folder(FolderRequest),
        default_group(DefaultGroupRequest),
        default_colaborator_group(ParentRequest),
        default_consumer_group(ParentRequest),
        default_administrator_group(ParentRequest),
        doc_lib(TenantRequest),
        home(HomeRequest),
        doc_libs_types(EmptyRequest),
        by_type(HomeRequest),
        by_path(ByPathRequest),
        node_child(NodeChildRequest),
        add_secondary_parent(SecondaryParentRequest),
        remove_secondary_parent(SecondaryParentRequest),
    );
}

#[derive(Default)]
struct MockTransfer;

#[tonic::async_trait]
impl BinaryTransferService for MockTransfer {
    async fn upload(
        &self,
        r: Request<tonic::Streaming<ChunkRequest>>,
    ) -> Result<Response<TransferReply>, Status> {
        auth(&r)?;
        let mut stream = r.into_inner();
        let mut bytes = 0usize;
        while let Some(msg) = stream.message().await? {
            if let Some(chunk_request::Payload::Chunk(c)) = msg.payload {
                bytes += c.len();
            }
        }
        Ok(Response::new(TransferReply {
            transfer_id: "t-1".into(),
            unique_id: "u-1".into(),
            status: "completed".into(),
            result: format!("{bytes} bytes"),
            duplicated: false,
        }))
    }
}

async fn spawn_mock() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(NodeManagerServiceServer::new(MockNm))
            .add_service(BinaryTransferServiceServer::new(MockTransfer))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    addr
}

/// Comando limpio: sin env vars del usuario que cambien endpoint/key/perfil.
fn alberto() -> Command {
    let mut cmd = Command::cargo_bin("alberto").unwrap();
    for var in [
        "ALBERTO_GRPC_ENDPOINT",
        "ALBERTO_REST_URL",
        "ALBERTO_API_KEY",
        "ALBERTO_PROFILE",
        "ALBERTO_CONFIG",
    ] {
        cmd.env_remove(var);
    }
    cmd.env("ALBERTO_CONFIG", "/nonexistent/alberto-config.toml");
    cmd
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn node_get_happy() {
    let addr = spawn_mock().await;
    alberto()
        .args(["node", "get", "abc", "--endpoint", &format!("http://{addr}"), "--api-key", KEY])
        .assert()
        .success()
        .stdout(contains(r#""unique_id": "abc""#));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn node_user_business_error() {
    let addr = spawn_mock().await;
    alberto()
        .args(["node", "user", "nadie", "--endpoint", &format!("http://{addr}"), "--api-key", KEY])
        .assert()
        .failure()
        .stderr(contains("{:error, not_found}"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bad_api_key_is_unauthenticated() {
    let addr = spawn_mock().await;
    alberto()
        .args(["node", "get", "abc", "--endpoint", &format!("http://{addr}"), "--api-key", "mala"])
        .assert()
        .failure()
        .stderr(contains("x-api-key invalida"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tenant_get_happy() {
    let addr = spawn_mock().await;
    alberto()
        .args(["tenant", "get", "acme", "--endpoint", &format!("http://{addr}"), "--api-key", KEY])
        .assert()
        .success()
        .stdout(contains(r#""tenant": "acme""#));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn admin_indexs_happy() {
    let addr = spawn_mock().await;
    alberto()
        .args(["admin", "indexs", "--endpoint", &format!("http://{addr}"), "--api-key", KEY])
        .assert()
        .success()
        .stdout(contains("rut"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn download_writes_file() {
    let addr = spawn_mock().await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("out.pdf");
    alberto()
        .args([
            "download",
            "abc",
            dest.to_str().unwrap(),
            "--endpoint",
            &format!("http://{addr}"),
            "--api-key",
            KEY,
        ])
        .assert()
        .success();
    assert_eq!(std::fs::read(&dest).unwrap(), b"%PDF-fake-content");
}

/// Camino de error por familia: la auth compartida rechaza en todas.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bad_api_key_fails_every_family() {
    let addr = spawn_mock().await;
    let ep = format!("http://{addr}");
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("f.txt");
    std::fs::write(&file, b"x").unwrap();

    let cases: Vec<Vec<&str>> = vec![
        vec!["tenant", "get", "acme"],
        vec!["admin", "indexs"],
        vec!["download", "abc"],
        vec!["upload", file.to_str().unwrap(), "--type", "t", "--parent", "p", "--user", "u", "--retries", "1"],
    ];
    for args in cases {
        alberto()
            .args(&args)
            .args(["--endpoint", &ep, "--api-key", "mala"])
            .assert()
            .failure();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn upload_happy() {
    let addr = spawn_mock().await;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("f.txt");
    std::fs::write(&file, b"hola mundo").unwrap();
    alberto()
        .args([
            "upload",
            file.to_str().unwrap(),
            "--type",
            "factura",
            "--parent",
            "p-1",
            "--user",
            "oscar",
            "--endpoint",
            &format!("http://{addr}"),
            "--api-key",
            KEY,
        ])
        .assert()
        .success()
        .stdout(contains("completed"));
}
```

- [ ] **Step 3: Run — expect all green**

Run: `cargo test`
Expected: 2 unit tests + 7 integration tests pass. (These tests pin current behavior; they are the safety net for Phases 1–2.)

- [ ] **Step 4: Verify gates + commit**

Run: `cargo fmt && cargo clippy --all-targets -- -D warnings`

```bash
git add Cargo.toml Cargo.lock tests/cli.rs
git commit -m "test: mock gRPC server in-process + e2e por familia de comandos"
```

### Task 10: TUI reuses one gRPC connection

**Files:**
- Modify: `src/tui.rs`

**Interfaces:**
- Consumes: `client::{nm_client, with_key}`.
- Produces: `App { client, api_key, ... }`; `fetch_doclib/fetch_children/fetch_content` take `(&mut NodeManagerServiceClient<Channel>, api_key: &str, ...)`. Tasks 11–12 build on these signatures.

- [ ] **Step 1: Restructure `App` and the fetchers**

In `src/tui.rs`:

```rust
use tonic::transport::Channel;
use crate::client::nodemanager::node_manager_service_client::NodeManagerServiceClient;

struct App {
    client: NodeManagerServiceClient<Channel>,
    api_key: String,
    levels: Vec<Level>,
    preview: Option<Preview>,
    status: String,
}
```

`run()` connects once:

```rust
pub fn run(tenant: String, grpc: GrpcOpts) -> Result<()> {
    let mut client = block_on(nm_client(&grpc))?;
    let api_key = grpc.api_key.clone();

    let doclib = block_on(fetch_doclib(&mut client, &api_key, &tenant))?;
    let doclib_id = doclib["unique_id"].as_str().unwrap_or_default().to_string();
    let nodes = block_on(fetch_children(&mut client, &api_key, &doclib_id))?;

    let mut app = App {
        client,
        api_key,
        levels: vec![level(format!("doclib {tenant}"), nodes)],
        preview: None,
        status: "↑↓ mover · Enter entrar/preview · Backspace subir · p preview · d descargar · q salir".into(),
    };
    // ... resto igual
}
```

Fetchers change signature (bodies otherwise identical, minus the per-call `nm_client`):

```rust
async fn fetch_doclib(
    c: &mut NodeManagerServiceClient<Channel>,
    api_key: &str,
    tenant: &str,
) -> Result<Value> {
    let req = nodemanager::TenantRequest { tenant: tenant.to_string() };
    let reply = c.doc_lib(with_key(req, api_key)?).await?.into_inner();
    if !reply.ok {
        bail!("doclib de '{tenant}': {}", reply.error);
    }
    Ok(serde_json::from_str(&reply.result_json)?)
}

async fn fetch_children(
    c: &mut NodeManagerServiceClient<Channel>,
    api_key: &str,
    unique_id: &str,
) -> Result<Vec<Value>> {
    let req = nodemanager::NodeChildRequest { unique_id: unique_id.to_string(), secondary: false };
    let reply = c.node_child(with_key(req, api_key)?).await?.into_inner();
    if !reply.ok {
        bail!("{}", reply.error);
    }
    parse_list(&reply.result_json)
}

async fn fetch_content(
    c: &mut NodeManagerServiceClient<Channel>,
    api_key: &str,
    unique_id: &str,
) -> Result<Vec<u8>> {
    let req = nodemanager::UniqueIdRequest { unique_id: unique_id.to_string() };
    let reply = c.node_content(with_key(req, api_key)?).await?.into_inner();
    if !reply.ok {
        bail!("{}", reply.error);
    }
    Ok(reply.content)
}
```

Call sites in `enter`, `open_preview`, `download_selected` become e.g.:

```rust
match block_on(fetch_children(&mut app.client, &app.api_key, &id)) {
```

(Disjoint field borrows of `app` are fine; clone the needed node fields before calling, as the current code already does.)

- [ ] **Step 2: Verify gates**

Run: `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`
Expected: all pass (TUI has no automated e2e; the compile + existing preview tests cover it).

- [ ] **Step 3: Commit**

```bash
git add src/tui.rs
git commit -m "tui: una sola conexión gRPC reutilizada en todo el navegador"
```

### Task 11: TUI real refresh

**Files:**
- Modify: `src/tui.rs`

**Interfaces:**
- Consumes: Task 10's `fetch_children(&mut client, &api_key, id)`.
- Produces: `Level { parent_id: String, ... }` — every level knows how to re-fetch itself.

- [ ] **Step 1: Add `parent_id` to `Level`**

```rust
struct Level {
    title: String,
    parent_id: String,
    nodes: Vec<Value>,
    state: ListState,
}

fn level(title: String, parent_id: String, nodes: Vec<Value>) -> Level {
    let mut state = ListState::default();
    if !nodes.is_empty() {
        state.select(Some(0));
    }
    Level { title, parent_id, nodes, state }
}
```

Update the two construction sites: in `run()` → `level(format!("doclib {tenant}"), doclib_id.clone(), nodes)`; in `enter()` → `level(name, id, nodes)` (note: `id` must be cloned before the fetch, which the current code already does).

- [ ] **Step 2: Implement refresh**

Replace the stub:

```rust
fn refresh(app: &mut App) {
    let Some(parent_id) = app.levels.last().map(|l| l.parent_id.clone()) else { return };
    let fetched = block_on(fetch_children(&mut app.client, &app.api_key, &parent_id));
    let lvl = app.levels.last_mut().unwrap();
    match fetched {
        Ok(nodes) => {
            let sel = lvl
                .state
                .selected()
                .unwrap_or(0)
                .min(nodes.len().saturating_sub(1));
            lvl.state.select(if nodes.is_empty() { None } else { Some(sel) });
            app.status = format!("refrescado: {} elementos", nodes.len());
            lvl.nodes = nodes;
        }
        Err(e) => app.status = format!("refresh: {e:#}"),
    }
}
```

Wait — `app.status` is assigned while `lvl` (a `&mut` into `app.levels`) is live; borrows of disjoint fields are legal, keep as written (`lvl` borrows `app.levels`, `app.status` is a different field).

Also update the status hint in `run()` to mention `r`:

```rust
status: "↑↓ mover · Enter entrar/preview · Backspace subir · p preview · d descargar · r refrescar · q salir".into(),
```

- [ ] **Step 3: Verify gates + commit**

Run: `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`

```bash
git add src/tui.rs
git commit -m "tui: r refresca el nivel actual (antes era un stub)"
```

### Task 12: TUI download filename sanitization (TDD)

**Files:**
- Modify: `src/tui.rs` (`download_selected` + tests module)

**Interfaces:**
- Produces: `fn sanitize_filename(name: &str) -> String` (private to tui).

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` in `src/tui.rs`:

```rust
#[test]
fn sanitize_quita_separadores_y_puntos_iniciales() {
    assert_eq!(sanitize_filename("../../etc/passwd"), "_.._etc_passwd");
    assert_eq!(sanitize_filename("informe 2026.pdf"), "informe 2026.pdf");
    assert_eq!(sanitize_filename(""), "descarga.bin");
    assert_eq!(sanitize_filename("..."), "descarga.bin");
    assert_eq!(sanitize_filename("a\\b/c"), "a_b_c");
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test sanitize`
Expected: FAIL — `cannot find function sanitize_filename`.

- [ ] **Step 3: Implement**

```rust
/// Nombre seguro para escribir en el directorio actual: sin separadores de
/// ruta y sin puntos iniciales (nada de ../, rutas absolutas ni ocultos).
fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | '\0') { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim_start_matches('.').trim();
    if trimmed.is_empty() {
        "descarga.bin".to_string()
    } else {
        trimmed.to_string()
    }
}
```

And in `download_selected`, replace:

```rust
let name = node["name"].as_str().unwrap_or("descarga.bin").to_string();
```

with:

```rust
let name = sanitize_filename(node["name"].as_str().unwrap_or("descarga.bin"));
```

and report the absolute path:

```rust
Ok(bytes) => {
    let size = bytes.len();
    let dest = std::env::current_dir().unwrap_or_default().join(&name);
    match std::fs::write(&dest, bytes) {
        Ok(()) => app.status = format!("descargado {} ({size} bytes)", dest.display()),
        Err(e) => app.status = format!("error escribiendo {}: {e}", dest.display()),
    }
}
```

- [ ] **Step 4: Run tests, verify pass, commit**

Run: `cargo test`
Expected: all pass.

```bash
git add src/tui.rs
git commit -m "tui: sanitizar nombre de descarga y reportar ruta absoluta"
```

---

## Phase 2 — Developer UX

### Task 13: Config profiles (TDD)

**Files:**
- Create: `src/config.rs`
- Modify: `Cargo.toml`, `src/lib.rs`, `src/cli.rs` (GrpcOpts + UploadArgs), `src/client.rs`, `src/commands/upload.rs`, `src/commands/download.rs`, `src/tui.rs`, `tests/cli.rs`

**Interfaces:**
- Produces:
  - `config::Config { default_profile: Option<String>, profiles: BTreeMap<String, Profile> }`, `config::Profile { endpoint: Option<String>, api_key: Option<String> }`
  - `config::config_path() -> PathBuf` (honors `ALBERTO_CONFIG` env override; default `~/.config/alberto/config.toml`)
  - `config::load() -> Result<Config>`, `config::load_from(&Path) -> Result<Config>`
  - `config::resolve(cfg, profile: Option<&str>, endpoint: Option<String>, api_key: Option<String>) -> Result<(String, String)>`
  - `cli::GrpcOpts` fields become `pub endpoint: Option<String>, pub api_key: Option<String>, pub profile: Option<String>` and gains `pub fn resolve(&self) -> Result<Conn>`; `cli::Conn { pub endpoint: String, pub api_key: String }`
  - `client::nm_client`/`nm_call` consume `&GrpcOpts` unchanged externally (they call `grpc.resolve()` internally).

- [ ] **Step 1: Add dependencies**

```toml
serde = { version = "1", features = ["derive"] }
toml = "0.8"
dirs = "6"
```

- [ ] **Step 2: Write `src/config.rs` with failing-first unit tests**

```rust
//! Perfiles de conexión: ~/.config/alberto/config.toml
//!
//! Precedencia: flag/env (clap los une) > --profile/ALBERTO_PROFILE >
//! default_profile del archivo > defaults de compilación.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:9090";

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    pub default_profile: Option<String>,
    #[serde(default)]
    pub profiles: BTreeMap<String, Profile>,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct Profile {
    pub endpoint: Option<String>,
    pub api_key: Option<String>,
}

pub fn config_path() -> PathBuf {
    if let Some(p) = std::env::var_os("ALBERTO_CONFIG") {
        return p.into();
    }
    dirs::home_dir().unwrap_or_default().join(".config/alberto/config.toml")
}

pub fn load() -> Result<Config> {
    load_from(&config_path())
}

pub fn load_from(path: &Path) -> Result<Config> {
    if !path.exists() {
        return Ok(Config::default());
    }
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("leyendo {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("TOML invalido en {}", path.display()))
}

/// Resuelve (endpoint, api_key) combinando flags/env con el perfil.
pub fn resolve(
    cfg: &Config,
    profile: Option<&str>,
    endpoint: Option<String>,
    api_key: Option<String>,
) -> Result<(String, String)> {
    let prof = match profile.or(cfg.default_profile.as_deref()) {
        Some(name) => Some(
            cfg.profiles
                .get(name)
                .with_context(|| {
                    format!("perfil '{name}' no existe en {}", config_path().display())
                })?
                .clone(),
        ),
        None => None,
    };

    let endpoint = endpoint
        .or_else(|| prof.as_ref().and_then(|p| p.endpoint.clone()))
        .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string());

    let Some(api_key) = api_key.or_else(|| prof.as_ref().and_then(|p| p.api_key.clone())) else {
        bail!(
            "falta el api key: usa --api-key, ALBERTO_API_KEY, o api_key en el perfil ({})",
            config_path().display()
        );
    };

    Ok((endpoint, api_key))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_with(default: Option<&str>, profiles: &[(&str, &str, &str)]) -> Config {
        Config {
            default_profile: default.map(String::from),
            profiles: profiles
                .iter()
                .map(|(n, e, k)| {
                    (
                        n.to_string(),
                        Profile {
                            endpoint: Some(e.to_string()),
                            api_key: Some(k.to_string()),
                        },
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn flag_gana_a_perfil() {
        let cfg = cfg_with(None, &[("qa", "http://qa:9090", "kqa")]);
        let (e, k) =
            resolve(&cfg, Some("qa"), Some("http://flag:1".into()), Some("kflag".into())).unwrap();
        assert_eq!(e, "http://flag:1");
        assert_eq!(k, "kflag");
    }

    #[test]
    fn perfil_llena_lo_que_falta() {
        let cfg = cfg_with(None, &[("qa", "http://qa:9090", "kqa")]);
        let (e, k) = resolve(&cfg, Some("qa"), None, None).unwrap();
        assert_eq!(e, "http://qa:9090");
        assert_eq!(k, "kqa");
    }

    #[test]
    fn default_profile_aplica_sin_flag() {
        let cfg = cfg_with(Some("qa"), &[("qa", "http://qa:9090", "kqa")]);
        let (e, k) = resolve(&cfg, None, None, None).unwrap();
        assert_eq!(e, "http://qa:9090");
        assert_eq!(k, "kqa");
    }

    #[test]
    fn perfil_inexistente_es_error() {
        let cfg = cfg_with(None, &[]);
        assert!(resolve(&cfg, Some("nope"), None, None).is_err());
    }

    #[test]
    fn sin_api_key_es_error_con_pista() {
        let cfg = Config::default();
        let err = resolve(&cfg, None, Some("http://x:1".into()), None).unwrap_err();
        assert!(err.to_string().contains("api key"));
    }

    #[test]
    fn endpoint_default_cuando_no_hay_nada() {
        let cfg = Config::default();
        let (e, _) = resolve(&cfg, None, None, Some("k".into())).unwrap();
        assert_eq!(e, DEFAULT_ENDPOINT);
    }

    #[test]
    fn load_from_lee_toml() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("c.toml");
        std::fs::write(
            &p,
            "default_profile = \"qa\"\n[profiles.qa]\nendpoint = \"http://qa:1\"\napi_key = \"s\"\n",
        )
        .unwrap();
        let cfg = load_from(&p).unwrap();
        assert_eq!(cfg.default_profile.as_deref(), Some("qa"));
        assert_eq!(cfg.profiles["qa"].endpoint.as_deref(), Some("http://qa:1"));
    }

    #[test]
    fn load_from_inexistente_es_default() {
        let cfg = load_from(Path::new("/no/existe/c.toml")).unwrap();
        assert!(cfg.profiles.is_empty());
    }
}
```

Add `pub mod config;` to `src/lib.rs`. Note: `tempfile` moves from `[dependencies]`… no — it is already a regular dependency (used by tui); usable in unit tests as-is.

- [ ] **Step 3: Run config tests**

Run: `cargo test config::`
Expected: all 8 pass (written together with impl; the "failing first" evidence is Step 2 of Task 14 exercising the CLI wiring).

- [ ] **Step 4: Rewire `GrpcOpts` (and `UploadArgs`) through resolution**

`src/cli.rs`:

```rust
/// Conexión ya resuelta (flags/env/perfil combinados).
pub struct Conn {
    pub endpoint: String,
    pub api_key: String,
}

#[derive(clap::Args, Clone)]
pub struct GrpcOpts {
    /// Endpoint gRPC (default: el del perfil, o http://127.0.0.1:9090)
    #[arg(long, env = "ALBERTO_GRPC_ENDPOINT")]
    pub endpoint: Option<String>,
    /// API key (metadata x-api-key)
    #[arg(long, env = "ALBERTO_API_KEY")]
    pub api_key: Option<String>,
    /// Perfil de ~/.config/alberto/config.toml
    #[arg(long, env = "ALBERTO_PROFILE")]
    pub profile: Option<String>,
}

impl GrpcOpts {
    pub fn resolve(&self) -> anyhow::Result<Conn> {
        let cfg = crate::config::load()?;
        let (endpoint, api_key) = crate::config::resolve(
            &cfg,
            self.profile.as_deref(),
            self.endpoint.clone(),
            self.api_key.clone(),
        )?;
        Ok(Conn { endpoint, api_key })
    }
}
```

`UploadArgs`: delete its `endpoint`/`api_key` fields and add `#[command(flatten)] pub grpc: GrpcOpts` (keep `retries`). `commands/upload.rs` starts with `let conn = a.grpc.resolve()?;` and passes `&conn.endpoint` / `&conn.api_key` to `upload_with_retries`.

`src/client.rs`: `nm_client` and `nm_call` resolve first —

```rust
pub async fn nm_client(
    grpc: &GrpcOpts,
) -> Result<(NodeManagerServiceClient<tonic::transport::Channel>, Conn)> {
    let conn = grpc.resolve()?;
    let channel = tonic::transport::Channel::from_shared(conn.endpoint.clone())
        // ... resto idéntico
    Ok((NodeManagerServiceClient::new(channel).max_decoding_message_size(1024 * 1024 * 1024), conn))
}
```

`nm_call` uses `let (client, conn) = nm_client(grpc).await?;` and `with_key(req, &conn.api_key)`. `commands/download.rs` and `src/tui.rs::run` do the same (`let (mut client, conn) = block_on(nm_client(&grpc))?; let api_key = conn.api_key;`).

- [ ] **Step 5: Verify everything still passes**

Run: `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`
Expected: all unit + integration tests pass (integration tests pass explicit flags, and `alberto()` pins `ALBERTO_CONFIG` to a nonexistent path, so resolution falls through to flags).

- [ ] **Step 6: Commit**

```bash
git add -A src Cargo.toml Cargo.lock
git commit -m "feat: perfiles de conexión en ~/.config/alberto/config.toml"
```

### Task 14: `alberto config init|list|show`

**Files:**
- Create: `src/commands/config_cmd.rs`
- Modify: `src/cli.rs`, `src/commands/mod.rs`, `src/main.rs`, `tests/cli.rs`

**Interfaces:**
- Consumes: `config::{load_from, config_path, Config}`.
- Produces: `Cmd::Config { cmd: ConfigCmd }` with `ConfigCmd::{Init, List, Show { profile: Option<String> }}`.

- [ ] **Step 1: Write the failing integration tests**

Append to `tests/cli.rs`:

```rust
#[test]
fn config_init_crea_archivo_y_list_lo_muestra() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("config.toml");

    let mut cmd = Command::cargo_bin("alberto").unwrap();
    cmd.env("ALBERTO_CONFIG", &cfg).args(["config", "init"]).assert().success();
    assert!(cfg.exists());

    let mut cmd = Command::cargo_bin("alberto").unwrap();
    cmd.env("ALBERTO_CONFIG", &cfg)
        .args(["config", "list"])
        .assert()
        .success()
        .stdout(contains("local"));
}

#[test]
fn config_show_enmascara_api_key() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("config.toml");
    std::fs::write(
        &cfg,
        "default_profile = \"qa\"\n[profiles.qa]\nendpoint = \"http://qa:1\"\napi_key = \"supersecreta\"\n",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("alberto").unwrap();
    cmd.env("ALBERTO_CONFIG", &cfg)
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(contains("supe…").and(contains("supersecreta").not()));
}
```

Add `use predicates::prelude::*;` to the imports (for `.and()`/`.not()`).

- [ ] **Step 2: Run to verify failure**

Run: `cargo test config_`
Expected: FAIL — clap error `unrecognized subcommand 'config'`.

- [ ] **Step 3: Implement**

`src/cli.rs` — add to `Cmd`:

```rust
/// Manejo del archivo de configuración (~/.config/alberto/config.toml)
Config {
    #[command(subcommand)]
    cmd: ConfigCmd,
},
```

and:

```rust
#[derive(Subcommand)]
pub enum ConfigCmd {
    /// Crea un config de ejemplo si no existe
    Init,
    /// Lista los perfiles configurados
    List,
    /// Muestra los valores efectivos de un perfil (api_key enmascarada)
    Show {
        /// Perfil a mostrar (default: default_profile)
        profile: Option<String>,
    },
}
```

`src/commands/config_cmd.rs`:

```rust
//! `alberto config *` — init/list/show del archivo de perfiles.

use anyhow::{bail, Context, Result};

use crate::cli::ConfigCmd;
use crate::config::{config_path, load, DEFAULT_ENDPOINT};

const TEMPLATE: &str = r#"default_profile = "local"

[profiles.local]
endpoint = "http://127.0.0.1:9090"
api_key = ""
"#;

pub fn run(cmd: ConfigCmd) -> Result<()> {
    match cmd {
        ConfigCmd::Init => {
            let path = config_path();
            if path.exists() {
                bail!("ya existe: {}", path.display());
            }
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)
                    .with_context(|| format!("creando {}", dir.display()))?;
            }
            std::fs::write(&path, TEMPLATE)
                .with_context(|| format!("escribiendo {}", path.display()))?;
            eprintln!("creado: {}", path.display());
            Ok(())
        }
        ConfigCmd::List => {
            let cfg = load()?;
            for (name, _) in &cfg.profiles {
                let marker =
                    if cfg.default_profile.as_deref() == Some(name) { " (default)" } else { "" };
                println!("{name}{marker}");
            }
            Ok(())
        }
        ConfigCmd::Show { profile } => {
            let cfg = load()?;
            let name = profile
                .or(cfg.default_profile.clone())
                .context("no hay perfil: pasa uno o define default_profile")?;
            let p = cfg
                .profiles
                .get(&name)
                .with_context(|| format!("perfil '{name}' no existe"))?;
            println!("perfil:   {name}");
            println!("endpoint: {}", p.endpoint.as_deref().unwrap_or(DEFAULT_ENDPOINT));
            println!("api_key:  {}", mask(p.api_key.as_deref().unwrap_or("")));
            Ok(())
        }
    }
}

fn mask(key: &str) -> String {
    if key.is_empty() {
        "(sin definir)".into()
    } else if key.len() <= 4 {
        "…".into()
    } else {
        format!("{}…", &key[..4])
    }
}
```

Wire up: `pub mod config_cmd;` in `commands/mod.rs`; in `main.rs` add `Cmd::Config { cmd } => commands::config_cmd::run(cmd),`.

- [ ] **Step 4: Run tests, verify pass, gates, commit**

Run: `cargo test && cargo fmt && cargo clippy --all-targets -- -D warnings`

```bash
git add -A src tests/cli.rs
git commit -m "feat: alberto config init|list|show"
```

### Task 15: Output modes (`--output pretty|json|raw|table`) (TDD)

**Files:**
- Modify: `src/cli.rs` (GrpcOpts), `src/client.rs`, `tests/cli.rs`

**Interfaces:**
- Produces:
  - `cli::Output` (clap `ValueEnum`): `Pretty | Json | Raw | Table`; field `pub output: Output` on `GrpcOpts`.
  - `pub fn client::format_result(result_json: &str, mode: Output) -> String` (pure, unit-tested).
  - `print_monadic(reply, mode)` — signature gains the mode.

- [ ] **Step 1: Write failing unit tests in `src/client.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Output;

    #[test]
    fn raw_pasa_tal_cual() {
        assert_eq!(format_result("{\"a\": 1}", Output::Raw), "{\"a\": 1}");
    }

    #[test]
    fn json_compacta() {
        assert_eq!(format_result("{ \"a\" : 1 }", Output::Json), "{\"a\":1}");
    }

    #[test]
    fn table_lista_columnas() {
        let json = r#"[{"unique_id":"u1","name":"a.pdf","type":"factura","content":true},
                       {"unique_id":"u2","name":"b","type":"folder","content":false}]"#;
        let t = format_result(json, Output::Table);
        let lines: Vec<&str> = t.lines().collect();
        assert!(lines[0].contains("unique_id") && lines[0].contains("name"));
        assert!(lines[1].contains("u1") && lines[1].contains("a.pdf"));
        assert!(lines[2].contains("u2"));
    }

    #[test]
    fn table_no_lista_cae_a_pretty() {
        let t = format_result("{\"a\":1}", Output::Table);
        assert!(t.contains("\"a\": 1"));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test client::tests`
Expected: FAIL — `format_result` not found / `Output` not found.

- [ ] **Step 3: Implement**

`src/cli.rs`:

```rust
/// Formato de salida para respuestas monádicas.
#[derive(clap::ValueEnum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Output {
    /// JSON indentado (default)
    Pretty,
    /// JSON compacto en una línea (para pipes)
    Json,
    /// result_json tal cual llegó del servidor
    Raw,
    /// Tabla (solo para resultados que son listas)
    Table,
}
```

Add to `GrpcOpts`:

```rust
/// Formato de salida
#[arg(long, value_enum, default_value_t = Output::Pretty)]
pub output: Output,
```

`src/client.rs`:

```rust
use crate::cli::Output;
use serde_json::Value;

pub fn format_result(result_json: &str, mode: Output) -> String {
    match mode {
        Output::Raw => result_json.to_string(),
        Output::Json => serde_json::from_str::<Value>(result_json)
            .map(|v| v.to_string())
            .unwrap_or_else(|_| result_json.to_string()),
        Output::Pretty => serde_json::from_str::<Value>(result_json)
            .and_then(|v| serde_json::to_string_pretty(&v))
            .unwrap_or_else(|_| result_json.to_string()),
        Output::Table => format_table(result_json),
    }
}

const TABLE_COLS: [&str; 4] = ["unique_id", "name", "type", "content"];

fn format_table(result_json: &str) -> String {
    let Ok(Value::Array(rows)) = serde_json::from_str::<Value>(result_json) else {
        return format_result(result_json, Output::Pretty);
    };

    let cell = |row: &Value, col: &str| match &row[col] {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };

    let mut widths: Vec<usize> = TABLE_COLS.iter().map(|c| c.len()).collect();
    let table: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            TABLE_COLS
                .iter()
                .enumerate()
                .map(|(i, col)| {
                    let v = cell(row, col);
                    widths[i] = widths[i].max(v.len());
                    v
                })
                .collect()
        })
        .collect();

    let fmt_row = |cells: &[String], widths: &[usize]| -> String {
        cells
            .iter()
            .zip(widths.iter().copied())
            .map(|(c, w)| format!("{c:<w$}"))
            .collect::<Vec<_>>()
            .join("  ")
            .trim_end()
            .to_string()
    };

    let header: Vec<String> = TABLE_COLS.iter().map(|s| s.to_string()).collect();
    let mut out = vec![fmt_row(&header, &widths)];
    out.extend(table.iter().map(|r| fmt_row(r, &widths)));
    out.join("\n")
}
```

`print_monadic` becomes:

```rust
pub fn print_monadic(reply: nodemanager::MonadicReply, mode: Output) -> Result<()> {
    if reply.ok {
        println!("{}", format_result(&reply.result_json, mode));
        Ok(())
    } else {
        bail!("{{:error, {}}}", reply.error);
    }
}
```

`nm_call` passes `grpc.output`. (TUI keeps its own JSON pretty pane — unaffected.)

- [ ] **Step 4: Add one integration test**

Append to `tests/cli.rs`:

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn node_get_output_raw() {
    let addr = spawn_mock().await;
    alberto()
        .args([
            "node", "get", "abc",
            "--endpoint", &format!("http://{addr}"),
            "--api-key", KEY,
            "--output", "raw",
        ])
        .assert()
        .success()
        .stdout(contains(r#"{"unique_id":"abc","name":"doc.pdf","content":true}"#));
}
```

- [ ] **Step 5: Run all tests, gates, commit**

Run: `cargo test && cargo fmt && cargo clippy --all-targets -- -D warnings`

```bash
git add -A src tests/cli.rs
git commit -m "feat: --output pretty|json|raw|table"
```

### Task 16: Shell completions

**Files:**
- Modify: `Cargo.toml`, `src/cli.rs`, `src/main.rs`, `tests/cli.rs`

- [ ] **Step 1: Failing test**

Append to `tests/cli.rs`:

```rust
#[test]
fn completions_zsh() {
    Command::cargo_bin("alberto")
        .unwrap()
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(contains("#compdef alberto"));
}
```

Run: `cargo test completions` → FAIL (`unrecognized subcommand`).

- [ ] **Step 2: Implement**

`Cargo.toml`: add `clap_complete = "4"`.

`src/cli.rs` — add to `Cmd`:

```rust
/// Genera autocompletado para tu shell (agrega a tu rc)
Completions {
    /// Shell destino
    #[arg(value_enum)]
    shell: clap_complete::Shell,
},
```

`src/main.rs`:

```rust
Cmd::Completions { shell } => {
    let mut cmd = <Cli as clap::CommandFactory>::command();
    clap_complete::generate(shell, &mut cmd, "alberto", &mut std::io::stdout());
    Ok(())
}
```

- [ ] **Step 3: Verify + commit**

Run: `cargo test && cargo fmt && cargo clippy --all-targets -- -D warnings`

```bash
git add Cargo.toml Cargo.lock src tests/cli.rs
git commit -m "feat: alberto completions <shell>"
```

### Task 17: Friendly error hints

**Files:**
- Modify: `src/client.rs`, `src/main.rs`, `tests/cli.rs`

**Interfaces:**
- Produces: `pub fn client::friendly(e: anyhow::Error) -> anyhow::Error` — applied once, at the top level in `main.rs`.

- [ ] **Step 1: Failing tests**

Unit test in `src/client.rs` tests module:

```rust
#[test]
fn hint_para_unauthenticated() {
    let e = anyhow::Error::new(tonic::Status::unauthenticated("x"));
    let msg = format!("{:#}", friendly(e));
    assert!(msg.contains("api key"));
}

#[test]
fn hint_para_connection_refused() {
    let e = anyhow::anyhow!("no se pudo conectar al endpoint gRPC");
    let msg = format!("{:#}", friendly(e));
    assert!(msg.contains("port-forward"));
}
```

Integration test (replace the body of `bad_api_key_is_unauthenticated`'s final assert):

```rust
        .stderr(contains("x-api-key invalida").and(contains("pista:")));
```

Run: `cargo test hint` → FAIL (`friendly` not found).

- [ ] **Step 2: Implement**

`src/client.rs`:

```rust
/// Envuelve errores comunes con una pista accionable.
pub fn friendly(e: anyhow::Error) -> anyhow::Error {
    match hint_for(&e) {
        Some(hint) => e.context(hint),
        None => e,
    }
}

fn hint_for(e: &anyhow::Error) -> Option<&'static str> {
    if let Some(status) = e.downcast_ref::<tonic::Status>() {
        return match status.code() {
            tonic::Code::Unauthenticated => {
                Some("pista: revisa el api key (--api-key, ALBERTO_API_KEY o el perfil)")
            }
            tonic::Code::DeadlineExceeded => {
                Some("pista: el servidor no respondió a tiempo — ¿endpoint correcto?")
            }
            _ => None,
        };
    }
    let text = format!("{e:#}");
    if text.contains("no se pudo conectar") || text.contains("Connection refused") {
        return Some(
            "pista: ¿está corriendo el port-forward? (kubectl port-forward svc/nodeservice 9090:9090)",
        );
    }
    None
}
```

`src/main.rs` — wrap the dispatch so the hint is outermost:

```rust
#[tokio::main]
async fn main() {
    let result = run().await;
    if let Err(e) = result {
        eprintln!("Error: {:#}", alberto_cli::client::friendly(e));
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    match Cli::parse().cmd {
        // ... el match existente, sin cambios
    }
}
```

- [ ] **Step 3: Verify + commit**

Run: `cargo test && cargo fmt && cargo clippy --all-targets -- -D warnings`

```bash
git add src tests/cli.rs
git commit -m "feat: pistas accionables en errores comunes (conexión, api key)"
```

---

## Phase 3 — Distribution

### Task 18: cargo-dist (binaries, shell installer, Homebrew tap)

**Files:**
- Create: `dist-workspace.toml`, `.github/workflows/release.yml` (generated)

- [ ] **Step 1: Install and init**

```bash
cargo install cargo-dist --locked
cargo dist init --yes \
  --installer=shell --installer=homebrew \
  --target=aarch64-apple-darwin --target=x86_64-apple-darwin \
  --target=x86_64-unknown-linux-gnu --target=aarch64-unknown-linux-gnu
```

(If the installed version's command is `dist` instead of `cargo dist`, use `dist init` with the same flags — same tool, renamed in newer releases.)

- [ ] **Step 2: Configure the tap**

Edit the generated `dist-workspace.toml` `[dist]` section to include:

```toml
tap = "GH_USER/homebrew-tap"
publish-jobs = ["homebrew"]
```

- [ ] **Step 3: Create the tap repo and token**

```bash
gh repo create homebrew-tap --public --description "Homebrew tap de GH_USER" \
  --add-readme
```

USER ACTION: create a fine-grained PAT with `contents: write` on `GH_USER/homebrew-tap` at https://github.com/settings/personal-access-tokens/new, then:

```bash
gh secret set HOMEBREW_TAP_TOKEN --repo GH_USER/alberto-cli
```

(paste the token when prompted — or ask the user to run `! gh secret set HOMEBREW_TAP_TOKEN --repo GH_USER/alberto-cli` so the token never enters the transcript).

- [ ] **Step 4: Verify the plan**

Run: `cargo dist plan`
Expected: lists artifacts for the 4 targets + `alberto-cli-installer.sh` + Homebrew formula, no errors.

- [ ] **Step 5: Commit**

```bash
git add dist-workspace.toml .github/workflows/release.yml Cargo.toml Cargo.lock
git commit -m "dist: cargo-dist (binarios mac/linux, installer sh, homebrew tap)"
```

### Task 19: .deb / .rpm packages via nfpm

**Files:**
- Create: `nfpm.yaml`, `.github/workflows/packages.yml`

(Separate workflow on `release: published` so regenerating cargo-dist's `release.yml` never clobbers it.)

- [ ] **Step 1: nfpm.yaml**

```yaml
name: alberto-cli
arch: amd64
platform: linux
version: ${SEMVER}
maintainer: Oscar Daniel Torres Hernández <odtorres891118@gmail.com>
description: >-
  CLI para NodeService: upload por gRPC streaming, operaciones de nodos y
  TUI con preview de PDFs en la terminal.
homepage: https://github.com/GH_USER/alberto-cli
license: MIT OR Apache-2.0
recommends:
  - poppler-utils
contents:
  - src: ./alberto
    dst: /usr/bin/alberto
```

- [ ] **Step 2: packages.yml**

```yaml
name: Linux packages

on:
  release:
    types: [published]

permissions:
  contents: write

jobs:
  nfpm:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Download release tarball
        env:
          GH_TOKEN: ${{ github.token }}
        run: |
          gh release download "${{ github.event.release.tag_name }}" \
            --pattern '*x86_64-unknown-linux-gnu.tar.*' --output linux.tar.xz
          tar -xf linux.tar.xz --strip-components=1
      - name: Install nfpm
        env:
          GH_TOKEN: ${{ github.token }}
        run: |
          gh release download -R goreleaser/nfpm --pattern '*_amd64.deb' --output nfpm.deb
          sudo dpkg -i nfpm.deb
      - name: Package deb + rpm
        env:
          SEMVER: ${{ github.event.release.tag_name }}
        run: |
          export SEMVER="${SEMVER#v}"
          envsubst '${SEMVER}' < nfpm.yaml > nfpm-resolved.yaml
          nfpm package -f nfpm-resolved.yaml -p deb
          nfpm package -f nfpm-resolved.yaml -p rpm
      - name: Upload to release
        env:
          GH_TOKEN: ${{ github.token }}
        run: gh release upload "${{ github.event.release.tag_name }}" ./*.deb ./*.rpm
```

- [ ] **Step 3: Validate nfpm.yaml locally (best effort)**

If nfpm is installed locally (`brew install nfpm`): build a release binary and dry-run:

```bash
cargo build --release && cp target/release/alberto .
SEMVER=0.0.0 envsubst '${SEMVER}' < nfpm.yaml > /tmp/nfpm-resolved.yaml
nfpm package -f /tmp/nfpm-resolved.yaml -p deb && rm -f ./alberto ./*.deb
```

Expected: a .deb is produced. If nfpm isn't available locally, skip — the workflow run in Task 21 is the real verification.

- [ ] **Step 4: Commit**

```bash
git add nfpm.yaml .github/workflows/packages.yml
git commit -m "dist: paquetes .deb/.rpm vía nfpm al publicar release"
```

### Task 20: README overhaul

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Rewrite README.md**

Structure (keep ALL existing Spanish content as the second half, under `## Documentación (español)`; the manual link `docs/manual-alberto-cli.md` stays):

```markdown
# alberto-cli 🦀

[![CI](https://github.com/GH_USER/alberto-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/GH_USER/alberto-cli/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/alberto-cli.svg)](https://crates.io/crates/alberto-cli)

Terminal client for **NodeService**: streaming gRPC uploads with idempotent
retries, 28 node-management operations, and an interactive TUI that previews
PDFs directly in your terminal.

## Install

**Homebrew (macOS/Linux)**
    brew install GH_USER/tap/alberto-cli

**Cargo**
    cargo install alberto-cli

**Shell installer**
    curl -fsSL https://github.com/GH_USER/alberto-cli/releases/latest/download/alberto-cli-installer.sh | sh

**Debian/Ubuntu & RPM**: download the .deb / .rpm from the
[latest release](https://github.com/GH_USER/alberto-cli/releases/latest).

PDF preview in the TUI requires poppler (`brew install poppler` /
`apt install poppler-utils`).

## Quick start

    # one-time: create a connection profile
    alberto config init
    ${EDITOR:-vi} ~/.config/alberto/config.toml   # set endpoint + api_key

    # upload a document
    alberto upload factura.pdf --type factura --parent <parent-id> --user oscar

    # inspect nodes
    alberto node get <unique-id>
    alberto node children <unique-id> --output table

    # browse interactively (with in-terminal PDF preview)
    alberto tui --tenant acme

## Documentación (español)

[... existing Spanish README content, verbatim ...]

Manual de usuario completo: [docs/manual-alberto-cli.md](docs/manual-alberto-cli.md)

## License

MIT OR Apache-2.0.
```

(Replace `GH_USER` with the real username; the indented blocks are fenced code blocks in the actual file.)

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: README en inglés con instalación + quick start"
```

### Task 21: Release v0.2.0 — crates.io + GitHub Release

**Files:**
- Modify: `Cargo.toml` (version)

- [ ] **Step 1: Bump version and push everything**

```bash
# Cargo.toml: version = "0.2.0"
cargo check
git add Cargo.toml Cargo.lock
git commit -m "release: v0.2.0"
git push origin development
git checkout main && git merge --ff-only development && git push origin main
git checkout development
```

- [ ] **Step 2: Verify CI green on main**

Run: `gh run watch $(gh run list --branch main --limit 1 --json databaseId -q '.[0].databaseId')`

- [ ] **Step 3: Publish to crates.io**

USER ACTION: `! cargo login` (token from https://crates.io/settings/tokens — keeps it out of the transcript). Then:

```bash
cargo publish --dry-run
cargo publish
```

Expected: `packaging` + `verifying` succeed; crate visible at https://crates.io/crates/alberto-cli.

- [ ] **Step 4: Tag and watch the release pipeline**

```bash
git tag v0.2.0 main
git push origin v0.2.0
gh run watch $(gh run list --workflow=release.yml --limit 1 --json databaseId -q '.[0].databaseId')
```

Expected: release workflow green → GitHub Release `v0.2.0` with 4 target tarballs + installer.sh; `packages.yml` then attaches .deb/.rpm; the Homebrew formula lands in `GH_USER/homebrew-tap`.

- [ ] **Step 5: End-to-end install verification**

```bash
brew install GH_USER/tap/alberto-cli && alberto --version   # expect: alberto 0.2.0
cargo install alberto-cli --root /tmp/alberto-install-check && /tmp/alberto-install-check/bin/alberto --version
```

Expected: both print `alberto 0.2.0`. This is the spec's success criterion — done.
