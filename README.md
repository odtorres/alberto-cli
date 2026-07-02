# alberto-cli 🦀

CLI en Rust para **NodeService**: sube archivos por el nuevo servicio **gRPC
streaming** (`upload_by_streaming_with_backpressure`) y consulta nodos vía REST.

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

**Todo** requiere API key (header `x-api-key`), igual que la capa HTTP:

- **Upload gRPC + download**: sirve una key de tenant (`nk_...`) o global.
- **`node get` / `node by-type`** (rutas `/internal/*`): requieren key **global**
  (tenant `"global"`), semántica heredada de la capa HTTP.

Se pasa con `--api-key` o env `ALBERTO_API_KEY`.

## Configuración

| Env | Default | Descripción |
|---|---|---|
| `ALBERTO_GRPC_ENDPOINT` | `http://127.0.0.1:9090` | Endpoint gRPC |
| `ALBERTO_REST_URL` | `http://127.0.0.1:3537` | Base REST de nodeservice |
| `ALBERTO_API_KEY` | — | API key (obligatoria) |

Los defaults asumen `kubectl port-forward svc/nodeservice-service 9090:9090`
(y `3537:3537` para los comandos REST).

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

### Consultas y descarga

```bash
# nodo por unique_id (key global)
alberto node get <UNIQUE_ID>

# nodos por tipo (key global)
alberto node by-type --type factura

# descarga del contenido binario
alberto download <UNIQUE_ID> --tenant totalcheck -o salida.pdf
```

## Salida y códigos de error

- Éxito → JSON en stdout, exit 0.
- Error de negocio (parent/usuario inexistente, key inválida) → mensaje en
  stderr, exit ≠ 0, **sin reintentos** (son permanentes).
- Error de red/timeout → reintenta solo con backoff; si se agota, exit ≠ 0.

## Contrato

`proto/binary_transfer.proto` es espejo de
`apps/nodeservice/priv/protos/binary_transfer.proto`. Si el contrato cambia en
el servidor, copiar el `.proto` actualizado aquí y recompilar.
