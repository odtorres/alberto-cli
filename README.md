# alberto-cli 🦀

> 📖 **Manual de usuario completo** (inicio rápido, recetas, troubleshooting):
> [`docs/manual-alberto-cli.md`](../../docs/manual-alberto-cli.md)

CLI en Rust para **NodeService**, 100% sobre los servicios gRPC nuevos:

- **`upload`** → `transfer.BinaryTransferService` (streaming con backpressure)
- **`node *`** → `nodemanager.NodeManagerService` (operaciones de nodos, respuesta monádica)
- **`download`** → gRPC `NodeContent` (descarga de contenido)

> Los endpoints HTTP viejos de upload quedan **excluidos** de este cliente por
> diseño: la única vía de subida es gRPC.

## Build

```bash
cd clients/alberto-cli
cargo build --release
# binario: target/release/alberto
```

Requiere `protoc` instalado (compila `proto/binary_transfer.proto` en build time).

## Autenticación

**Todo** requiere API key (metadata/header `x-api-key`), igual que la capa
HTTP. Sirve una key de tenant (`nk_...`) o una global (tenant `"global"`).
Se pasa con `--api-key` o env `ALBERTO_API_KEY`.

## Configuración

| Env | Default | Descripción |
|---|---|---|
| `ALBERTO_GRPC_ENDPOINT` | `http://127.0.0.1:9090` | Endpoint gRPC |
| `ALBERTO_API_KEY` | — | API key (obligatoria) |

El default asume `kubectl port-forward svc/nodeservice-service 9090:9090`.

## Uso

### Upload (gRPC streaming)

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

### Consultas de nodos (gRPC `NodeManagerService`)

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

### Descarga de contenido

```bash
# gRPC NodeContent; el destino es posicional (default: <unique_id>.bin).
# No requiere tenant: el unique_id es único en todo el repositorio.
alberto download <UNIQUE_ID> salida.pdf
```

## Salida y códigos de error

- Éxito → JSON en stdout, exit 0.
- Error de negocio (parent/usuario inexistente, key inválida) → mensaje en
  stderr, exit ≠ 0, **sin reintentos** (son permanentes).
- Error de red/timeout → reintenta solo con backoff; si se agota, exit ≠ 0.

## Contratos

`proto/binary_transfer.proto` y `proto/node_manager.proto` son espejos de
`apps/nodeservice/priv/protos/*.proto`. Si un contrato cambia en el servidor,
copiar el `.proto` actualizado aquí y recompilar (`cargo build --release`).
