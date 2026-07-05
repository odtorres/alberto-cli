# 🦀 Manual de Usuario — `alberto` CLI

> La herramienta de línea de comandos para trabajar con **NodeService**:
> sube documentos por gRPC streaming y consulta/modifica nodos — sin escribir
> una línea de código.

---

## 🚀 Inicio rápido (5 minutos)

### 1. Consigue el binario

```bash
git clone git@github.com:odtorres/alberto-cli.git
cd alberto-cli
cargo build --release
sudo cp target/release/alberto /usr/local/bin/   # opcional, para tenerlo en el PATH
```

> Requisitos: solo Rust (`rustup.rs`). `protoc` viene vendorizado
> (`protoc-bin-vendored`), no hace falta instalarlo en el sistema.

### 2. Configura tu entorno (una vez)

```bash
# En tu ~/.zshrc o ~/.bashrc
export ALBERTO_API_KEY="nk_tu-api-key-aqui"           # pídesela a tu admin
export ALBERTO_GRPC_ENDPOINT="http://127.0.0.1:9090"  # default
```

### 3. Abre el túnel a QA

```bash
kubectl port-forward svc/nodeservice 9090:9090
```

### 4. ¡Listo! Sube tu primer documento

```bash
alberto upload contrato.pdf \
  --type factura \
  --parent 5cd3399a-78da-11e8-b829-0a580a200520 \
  --user soportevn \
  --tenant totalcheck
```

```json
{"unique_id":"ed0936e2-...","transfer_id":"1ea4f290-...","status":"completed","duplicated":false}
```

El `unique_id` es tu documento en NodeService. 🎉

---

## 📖 Chuleta de comandos (TODAS las operaciones)

### Subida y descarga de contenido

| Quiero... | Comando |
|---|---|
| Subir un archivo | `alberto upload <archivo> --type T --parent P --user U --tenant X` |
| Subir + asociar a otro nodo | `... --assoc <UNIQUE_ID>` |
| Subir documento firmado | `... --signed-ref <UNIQUE_ID>` |
| Descargar contenido | `alberto download <UNIQUE_ID> [destino.pdf]` — sin tenant: el id es único global |
| **Navegar visualmente + preview de PDFs** | `alberto tui --tenant totalcheck` 🖼️ |

### Consultar nodos

| Quiero... | Comando | RPC |
|---|---|---|
| Ver un nodo por id | `alberto node get <UNIQUE_ID>` | NodeGet |
| Ver varios nodos de una vez | `alberto node ids <ID1> <ID2> [--type T]` | Ids |
| Buscar nodo por ruta | `alberto node by-path /documentlibrary/factura --tenant totalcheck` | ByPath |
| Buscar nodos por nombre | `alberto node by-name <nombre>` | NodeByName |
| Listar por tipo en un tenant | `alberto node by-type --type factura --tenant totalcheck` | ByType |
| Ver hijos de un nodo | `alberto node children <UNIQUE_ID>` | NodeChild |
| Ver hijos secundarios | `alberto node children <UNIQUE_ID> --secondary` | NodeChild |
| Leer un valor dentro del data | `alberto node get-in /tenants/t/doclib/x --path clave subclave` | Get (`:get_m`) |
| Buscar un usuario | `alberto node user <username>` | User |

### Modificar nodos

| Quiero... | Comando | RPC |
|---|---|---|
| Mezclar metadata (merge) | `alberto node datamerge <ID> --data '{"estado":"ok"}'` | Datamerge (`:datamerge_m`) |
| Merge estricto | `alberto node data-update <ID> --data '{...}'` | DataUpdate (`:dataupdate`) |
| Merge masivo (varios nodos) | `alberto node bulk-datamerge --changes '[...]'` | BulkDatamerge |
| Cambiar un valor por path | `alberto node patch /tenants/t/doclib/x --path clave --data '"valor"'` | Patch (`:patch_m`) |
| Crear nodo sin contenido | `alberto node create --parent <ID> --type T --data '{"name":"n"}'` | NodeCreate (`:node_m`) |
| Asociar secondary parent | `alberto node add-secondary <CHILD_ID> <PARENT_ID>` | AddSecondaryParent |
| Quitar secondary parent | `alberto node remove-secondary <CHILD_ID> <PARENT_ID>` | RemoveSecondaryParent |

### Tenants

| Quiero... | Comando | RPC |
|---|---|---|
| Ver el nodo del tenant | `alberto tenant get totalcheck` | TenantGet |
| Crear un tenant | `alberto tenant create t --title X --dni Y --company Z --email E` | TenantCreate |
| Document library del tenant | `alberto tenant doclib totalcheck` | DocLib |
| Home de un tipo documental | `alberto tenant home totalcheck --type factura` | Home |
| Package clasificado de un tipo | `alberto tenant package totalcheck --type factura` | Package (`:package_m`) |

### Administración

| Quiero... | Comando | RPC |
|---|---|---|
| Crear un folder | `alberto admin folder --parent <ID> --name n --title T` | Folder |
| Crear grupo default | `alberto admin default-group --name G --parent <ID>` | DefaultGroup |
| Grupo colaborador del tenant | `alberto admin colaborator-group --parent <ID>` | DefaultColaboratorGroup |
| Grupo consumidor del tenant | `alberto admin consumer-group --parent <ID>` | DefaultConsumerGroup |
| Grupo administrador del tenant | `alberto admin administrator-group --parent <ID>` | DefaultAdministratorGroup |
| Ver índices configurados (etcd) | `alberto admin indexs` | Indexs |
| Ver tipos documentales | `alberto admin doclib-types` | DocLibsTypes |

> 💡 `alberto --help`, `alberto node --help`, `alberto tenant --help`,
> `alberto admin --help` muestran todo con sus flags.

---

## 🖼️ El modo TUI (navegador con preview de PDFs)

```bash
alberto tui --tenant totalcheck
```

Un navegador de dos paneles dentro de tu terminal:

```
┌ doclib totalcheck / factura ────┐┌ 📄 factura_prueba.pdf — pág 1/3 ─┐
│  📁 2026                        ││ ▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄ │
│▶ 📄 factura_prueba.pdf          ││ █  FACTURA DE PRUEBA           █ │
│  📄 contrato_enero.pdf          ││ █  ───────────────────────    █ │
│                                 ││ █  Tenant: totalcheck ...      █ │
└─────────────────────────────────┘└──────────────────────────────────┘
 factura_prueba.pdf — página 1/3 · ←→ páginas · Esc cerrar
```

| Tecla | Acción |
|---|---|
| `↑` `↓` | Moverse por la lista |
| `Enter` | Entrar a carpeta 📁 / **preview** de documento 📄 |
| `Backspace` / `←` | Subir un nivel |
| `p` | Preview del seleccionado |
| `←` `→` | Página anterior / siguiente del PDF |
| `d` | Descargar el documento al directorio actual |
| `Esc` / `q` | Cerrar preview / salir |

**Requisitos**: `pdftoppm` (`pacman -S poppler` / `apt install poppler-utils` /
`brew install poppler`) y una terminal con truecolor (prácticamente todas:
kitty, alacritty, iTerm2, wezterm, gnome-terminal…). El PDF se rasteriza y se
pinta con半bloques RGB — no necesita protocolos gráficos especiales.

---

## 📤 Subir documentos en detalle

### Anatomía del upload

```bash
alberto upload informe.pdf \
  --type factura \                    # tipo documental (obligatorio)
  --title "Informe enero 2026" \      # título visible (default: nombre del archivo)
  --description "Cierre mensual" \    # opcional
  --parent <UNIQUE_ID> \              # dónde cuelga el documento (obligatorio)
  --user soportevn \                  # quién lo sube (obligatorio, debe existir)
  --tenant totalcheck \               # tenant
  --data '{"rut":"11.111.111-1","periodo":"2026-01"}'   # metadata JSON libre
```

### ¿No sabes el `--parent`? Búscalo por ruta:

```bash
alberto node by-path /documentlibrary/factura --tenant totalcheck
# el "unique_id" de la respuesta es tu --parent
```

### Las 3 variantes de subida

| Variante | Cuándo | Flag extra |
|---|---|---|
| **Simple** | Documento normal | (ninguno) |
| **Asociada** | El documento pertenece también a otro nodo (ej: una solicitud) | `--assoc <ID>` |
| **Firmada** | El documento referencia un contenido firmado digitalmente | `--signed-ref <ID>` |

### 💪 Superpoderes que trae de fábrica

- **Archivos gigantes sin miedo**: el archivo viaja en trozos de 64 KiB con
  barra de progreso — la RAM del servidor se mantiene plana aunque subas GBs.
- **Reintentos que no duplican**: si la red se corta, el CLI reintenta solo
  (3 veces). Gracias a la clave de idempotencia interna, aunque el primer
  intento SÍ hubiera llegado, el reintento devuelve **el mismo documento** —
  jamás verás duplicados por reintento. Si pasa, la respuesta trae
  `"duplicated": true`.
- **Errores claros**: si el `--parent` no existe, o el `--user` está mal, el
  error te lo dice y NO se reintenta (corriges y va).

---

## 🔍 Consultar y modificar nodos

Todos los comandos `node *` hablan gRPC con el `NodeManagerService` y comparten
una regla: la respuesta **siempre** es monádica.

- ✅ Éxito → JSON bonito por stdout, exit `0` — perfecto para pipes y scripts:
  ```bash
  alberto node get <ID> | jq -r .pathfs
  ```
- ❌ Error → `Error: {:error, not_found}` por stderr, exit `≠ 0`:
  ```bash
  alberto node get <ID> || echo "no existe, creando..."
  ```

### Recetas útiles

```bash
# ¿Qué facturas hay en el tenant?
alberto node by-type --type factura --tenant totalcheck | jq -r '.[] | .unique_id + "  " + .title'

# Marcar un documento como procesado
alberto node datamerge <ID> --data '{"estado":"procesado","fecha_proceso":"2026-07-02"}'

# Bajar todos los hijos de un nodo (ids) y descargarlos
for id in $(alberto node children <ID> | jq -r '.[].unique_id'); do
  alberto download $id "$id.pdf"
done

# Verificar la identidad de un usuario (password siempre enmascarada 🔒)
alberto node user soportevn | jq .data
```

---

## 🔑 API Keys

Todo requiere una API key (`nk_...`) — la misma que usa la capa HTTP:

| Tipo | Qué puede hacer |
|---|---|
| **Key de tenant** | Subir/consultar/modificar dentro de **su** tenant |
| **Key global** (`tenant: "global"`) | Todo, en cualquier tenant |

Si intentas subir a un tenant ajeno con key de tenant → `PERMISSION_DENIED`.
Pídele la tuya al administrador (se crean con `POST /internal/tenant/{t}/api-keys`).

---

## ⚙️ Perfiles, formatos de salida y autocompletado

### Perfiles de conexión

En vez de exportar `ALBERTO_GRPC_ENDPOINT`/`ALBERTO_API_KEY` a mano, guárdalos
en `~/.config/alberto/config.toml`:

```bash
alberto config init            # crea el archivo con un perfil "local" de ejemplo
alberto config list            # perfiles disponibles (marca el default)
alberto config show [perfil]   # endpoint + api_key enmascarada
```

Elige el perfil con `--profile qa` o `ALBERTO_PROFILE=qa`. Precedencia para
resolver endpoint/api_key:

```
flag (--endpoint/--api-key) > env (ALBERTO_GRPC_ENDPOINT/ALBERTO_API_KEY)
  > --profile / ALBERTO_PROFILE > default_profile del archivo
```

`ALBERTO_CONFIG=/ruta/otro.toml` cambia dónde se busca el archivo (útil en CI
o para tener un config distinto por proyecto).

### Formatos de salida (`--output`)

| Valor | Qué hace |
|---|---|
| `pretty` (default) | JSON indentado, legible |
| `json` | JSON compacto en una línea — ideal para pipes (`\| jq`) |
| `raw` | El `result_json` tal cual llegó del servidor |
| `table` | Tabla, solo cuando el resultado es una lista (ej. `node by-type`) |

```bash
alberto node by-type --type factura --tenant totalcheck --output table
```

### Autocompletado de shell

```bash
alberto completions zsh >> ~/.zshrc     # o bash / fish / elvish / powershell
```

---

## 🩺 Troubleshooting

| Síntoma | Causa y solución |
|---|---|
| `tcp connect error` / `Connection refused` | El túnel está caído. Relevanta el `kubectl port-forward`. **Ojo**: cada deploy a QA mata los túneles. |
| `status: Unauthenticated, "Invalid API key"` | Key mala o revocada. Revisa `ALBERTO_API_KEY`. |
| `status: Unauthenticated, "API key authentication required"` | No estás mandando key (¿env sin exportar?). |
| `PermissionDenied ... does not match transfer tenant` | Tu key es de otro tenant. |
| `Error: {:error, not_found}` | El `unique_id`/path/usuario no existe. No es un bug — es la respuesta monádica de error. |
| `Error: {:error, :user_not_found}` en upload | El `--user` no existe en NodeService. |
| Upload lento / timeout | Archivo muy grande por túnel. Sube `--retries` o corre el CLI más cerca del cluster (el server espera hasta 5 min por transfer). |
| `--data no es JSON valido` | Revisa comillas: usa `'{"clave":"valor"}'` (simples por fuera, dobles por dentro). |

### ¿Dónde miro los logs del servidor?

Cada upload devuelve un `transfer_id`. Con él:

```bash
kubectl logs -l app=nodeservice --prefix --since=30m | grep "transfer <TRANSFER_ID>"
```

---

## 🧭 ¿Qué pasa por dentro? (para curiosos)

```
alberto upload ──chunks 64KiB──▶ gRPC :9090 ──WAL disco──▶ GenStage (backpressure)
                                                              │
                       tu terminal ◀──unique_id── nodo ◀──────┴──▶ Google Cloud Storage
```

1. El CLI manda la metadata + el archivo en trozos (client streaming).
2. El servidor lo persiste a disco (Write-Ahead Log) con RAM plana.
3. Un pipeline GenStage lo sube a GCS en bloques con backpressure.
4. Se crea el nodo (igual que el flujo clásico: notificaciones e indexado incluidos).
5. Te llega el `unique_id` por la misma conexión. El WAL se borra.

Detalles completos del protocolo (para integrar otros lenguajes):
[`docs/integracion-grpc-streaming-upload.md`](integracion-grpc-streaming-upload.md)

---

## 📎 Referencias

| Recurso | Ubicación |
|---|---|
| Código del CLI | raíz de este repo (`alberto-cli/`) |
| README técnico del CLI | `README.md` |
| Manual de integración gRPC (todos los lenguajes) | `docs/integracion-grpc-streaming-upload.md` |
| Librería Elixir (para servicios) | `clients/alberto_upload_client/` (repo `umbrella_nodeservice`) |
| Contratos proto | `proto/*.proto` (espejo de `apps/nodeservice/priv/protos/*.proto` en `umbrella_nodeservice`) |
