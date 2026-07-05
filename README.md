# alberto-cli 🦀

[![CI](https://github.com/odtorres/alberto-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/odtorres/alberto-cli/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/alberto-cli.svg)](https://crates.io/crates/alberto-cli)

Terminal client for **NodeService**: streaming gRPC uploads with idempotent
retries, 28 node-management operations, and an interactive TUI that previews
PDFs directly in your terminal.

## Install

**Homebrew (macOS/Linux)**
```bash
brew install odtorres/tap/alberto-cli
```

**Cargo**
```bash
cargo install alberto-cli
```

**Shell installer**
```bash
curl -fsSL https://github.com/odtorres/alberto-cli/releases/latest/download/alberto-cli-installer.sh | sh
```

**Debian/Ubuntu & RPM**: download the .deb / .rpm from the
[latest release](https://github.com/odtorres/alberto-cli/releases/latest).

PDF preview in the TUI requires poppler (`brew install poppler` /
`apt install poppler-utils`).

## Quick start

```bash
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
```

## Documentación (español)

> 📖 **Manual de usuario completo** (inicio rápido, recetas, troubleshooting):
> [`docs/manual-alberto-cli.md`](docs/manual-alberto-cli.md)

CLI en Rust para **NodeService**, 100% sobre los servicios gRPC nuevos:

- **`upload`** → `transfer.BinaryTransferService` (streaming con backpressure)
- **`node *`** → `nodemanager.NodeManagerService` (operaciones de nodos, respuesta monádica)
- **`download`** → gRPC `NodeContent` (descarga de contenido)
- **`tui`** → navegador interactivo con **preview de PDFs en la terminal** 🖼️

> Los endpoints HTTP viejos de upload quedan **excluidos** de este cliente por
> diseño: la única vía de subida es gRPC.

### Build

```bash
cd clients/alberto-cli
cargo build --release
# binario: target/release/alberto
```

Requiere `protoc` instalado (compila `proto/binary_transfer.proto` en build time).

### Autenticación

**Todo** requiere API key (metadata/header `x-api-key`), igual que la capa
HTTP. Sirve una key de tenant (`nk_...`) o una global (tenant `"global"`).
Se pasa con `--api-key` o env `ALBERTO_API_KEY`.

### Configuración

| Env | Default | Descripción |
|---|---|---|
| `ALBERTO_GRPC_ENDPOINT` | `http://127.0.0.1:9090` | Endpoint gRPC |
| `ALBERTO_API_KEY` | — | API key (obligatoria) |

El default asume `kubectl port-forward svc/nodeservice-service 9090:9090`.

### Uso

#### Upload (gRPC streaming)

```bash
alberto upload factura.pdf \
  --type factura \
  --title "Factura enero" \
  --parent <UNIQUE_ID_PARENT> \
  --user soportevn \
  --tenant totalcheck \
  --data '{"rut":"11.111.111-1"}'
# → {"unique_id":"...","transfer_id":"...","status":"completed","duplicated":false}
```

Variantes (mutuamente excluyentes):

```bash
# assoc: asocia como secondary_parent
alberto upload doc.pdf ... --assoc <UNIQUE_ID_NODO_A_ASOCIAR>

# signed: referencia contenido firmado
alberto upload doc.pdf ... --signed-ref <UNIQUE_ID_CONTENIDO_FIRMADO>
```

Características automáticas (sin flags):

- **Chunks de 64 KiB** con barra de progreso — archivos de cualquier tamaño.
- **Idempotencia**: genera un `client_ref` interno por invocación; los
  reintentos (`--retries`, default 3) lo reutilizan → **jamás duplica** un
  documento aunque la red se corte tras completarse la subida.

#### Consultas de nodos (gRPC `NodeManagerService`)

Toda respuesta es **monádica**: `{:ok, valor}` → JSON en stdout, exit 0;
`{:error, razón}` → `Error: {:error, razón}` en stderr, exit ≠ 0.

```bash
# nodo por unique_id (NodeGet)
alberto node get <UNIQUE_ID>

# varios nodos por sus unique_ids en una sola llamada (Ids); --type filtra
alberto node ids <ID1> <ID2> <ID3>
alberto node ids <ID1> <ID2> --type factura

# nodo por path (ByPath): absoluto o relativo a un tenant
alberto node by-path /tenants/totalcheck/documentlibrary
alberto node by-path /documentlibrary/factura --tenant totalcheck

# nodos por tipo en un tenant (ByType)
alberto node by-type --type factura --tenant totalcheck

# hijos de un nodo (NodeChild); --secondary para secondary_parent
alberto node children <UNIQUE_ID>
alberto node children <UNIQUE_ID> --secondary

# usuario por username (User; password siempre enmascarada)
alberto node user soportevn

# mezclar data JSON en el nodo (Datamerge, :datamerge_m)
alberto node datamerge <UNIQUE_ID> --data '{"estado":"procesado"}'
```

#### TUI — navegador interactivo

```bash
alberto tui --tenant totalcheck
```

Navega el document library (↑↓/Enter/Backspace), muestra el detalle JSON del
nodo y **previsualiza PDFs directamente en la terminal** (Enter o `p` sobre un
📄; ←→ cambia de página, `d` descarga, Esc/q cierra). Requiere `pdftoppm`
(paquete `poppler`/`poppler-utils`) y terminal truecolor.

#### Descarga de contenido

```bash
# gRPC NodeContent; el destino es posicional (default: <unique_id>.bin).
# No requiere tenant: el unique_id es único en todo el repositorio.
alberto download <UNIQUE_ID> salida.pdf
```

### Salida y códigos de error

- Éxito → JSON en stdout, exit 0.
- Error de negocio (parent/usuario inexistente, key inválida) → mensaje en
  stderr, exit ≠ 0, **sin reintentos** (son permanentes).
- Error de red/timeout → reintenta solo con backoff; si se agota, exit ≠ 0.

### Contratos

`proto/binary_transfer.proto` y `proto/node_manager.proto` son espejos de
`apps/nodeservice/priv/protos/*.proto`. Si un contrato cambia en el servidor,
copiar el `.proto` actualizado aquí y recompilar (`cargo build --release`).

## License

MIT OR Apache-2.0. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE) for details.
