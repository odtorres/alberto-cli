//! alberto — CLI para NodeService.
//!
//! * `upload`   → gRPC client-streaming (feature upload_by_streaming_with_backpressure)
//!                con idempotencia (client_ref) y reintentos automáticos.
//! * `node`     → operaciones REST contra node_manager (get / children).
//! * `download` → descarga el contenido binario de un nodo vía REST.
//!
//! Endpoints por defecto (pensados para `kubectl port-forward`):
//!   gRPC: http://127.0.0.1:9090   (override: --endpoint o ALBERTO_GRPC_ENDPOINT)
//!   REST: http://127.0.0.1:3537  (override: --rest-url o ALBERTO_REST_URL)

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use tokio::io::AsyncReadExt;

pub mod transfer {
    tonic::include_proto!("transfer");
}

pub mod nodemanager {
    tonic::include_proto!("nodemanager");
}

use nodemanager::node_manager_service_client::NodeManagerServiceClient;
use transfer::binary_transfer_service_client::BinaryTransferServiceClient;
use transfer::{chunk_request::Payload, ChunkRequest, TransferMeta};

const CHUNK_SIZE: usize = 64 * 1024;

#[derive(Parser)]
#[command(
    name = "alberto",
    version,
    about = "CLI para NodeService: upload por gRPC streaming + operaciones de nodos"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Sube un archivo por gRPC streaming y crea el nodo (variantes plain/assoc/signed)
    Upload {
        /// Archivo a subir
        file: PathBuf,
        /// Tipo documental del nodo (ej: factura)
        #[arg(long = "type")]
        node_type: String,
        /// Título del nodo (default: nombre del archivo)
        #[arg(long)]
        title: Option<String>,
        /// Descripción
        #[arg(long, default_value = "")]
        description: String,
        /// unique_id del nodo padre (debe existir)
        #[arg(long)]
        parent: String,
        /// Username que sube (debe existir)
        #[arg(long)]
        user: String,
        /// Tenant (informativo; el efectivo se hereda del parent)
        #[arg(long, default_value = "")]
        tenant: String,
        /// Metadata JSON del nodo, ej: '{"rut":"1-9"}'
        #[arg(long, default_value = "{}")]
        data: String,
        /// unique_id a asociar como secondary_parent (activa variante assoc)
        #[arg(long)]
        assoc: Option<String>,
        /// unique_id del contenido firmado a referenciar (activa variante signed)
        #[arg(long)]
        signed_ref: Option<String>,
        /// Endpoint gRPC
        #[arg(long, env = "ALBERTO_GRPC_ENDPOINT", default_value = "http://127.0.0.1:9090")]
        endpoint: String,
        /// API key (header x-api-key, igual que la capa HTTP)
        #[arg(long, env = "ALBERTO_API_KEY")]
        api_key: String,
        /// Intentos totales ante fallas de red/timeout (la idempotencia evita duplicados)
        #[arg(long, default_value_t = 3)]
        retries: u32,
    },
    /// Operaciones de nodos vía gRPC NodeManagerService (respuesta monádica)
    Node {
        #[command(subcommand)]
        cmd: NodeCmd,
    },
    /// Operaciones de tenant vía gRPC NodeManagerService
    Tenant {
        #[command(subcommand)]
        cmd: TenantCmd,
    },
    /// Operaciones administrativas (folders, grupos, índices)
    Admin {
        #[command(subcommand)]
        cmd: AdminCmd,
    },
    /// Descarga el contenido binario de un nodo
    Download {
        /// unique_id del nodo
        id: String,
        #[arg(long)]
        tenant: String,
        /// Archivo de salida (default: nombre del nodo o <id>.bin)
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long, env = "ALBERTO_REST_URL", default_value = "http://127.0.0.1:3537")]
        rest_url: String,
        /// API key (header x-api-key)
        #[arg(long, env = "ALBERTO_API_KEY")]
        api_key: String,
    },
}

#[derive(clap::Args)]
struct GrpcOpts {
    /// Endpoint gRPC
    #[arg(long, env = "ALBERTO_GRPC_ENDPOINT", default_value = "http://127.0.0.1:9090")]
    endpoint: String,
    /// API key (metadata x-api-key)
    #[arg(long, env = "ALBERTO_API_KEY")]
    api_key: String,
}

#[derive(Subcommand)]
enum NodeCmd {
    /// Obtiene un nodo por unique_id (NodeGet)
    Get {
        id: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Obtiene varios nodos por sus unique_ids (Ids); --type filtra por tipo
    Ids {
        /// Lista de unique_ids separados por espacio
        #[arg(required = true, num_args = 1..)]
        ids: Vec<String>,
        #[arg(long = "type", default_value = "")]
        node_type: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Lista nodos por tipo dentro de un tenant (ByType)
    ByType {
        #[arg(long = "type")]
        node_type: String,
        #[arg(long)]
        tenant: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Obtiene un nodo por path (ByPath); --tenant opcional para path relativo
    ByPath {
        path: String,
        #[arg(long, default_value = "")]
        tenant: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Lista los hijos de un nodo (NodeChild); --secondary para secondary_parent
    Children {
        id: String,
        #[arg(long, default_value_t = false)]
        secondary: bool,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Busca un usuario por username (User; password siempre enmascarada)
    User {
        username: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Mezcla data JSON en el data del nodo (Datamerge, :datamerge_m)
    Datamerge {
        id: String,
        /// JSON a mezclar, ej: '{"estado":"procesado"}'
        #[arg(long)]
        data: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Merge estricto de data (DataUpdate, :dataupdate)
    DataUpdate {
        id: String,
        #[arg(long)]
        data: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Merge masivo de data en varios nodos (BulkDatamerge, :bulk_datamerge_m)
    BulkDatamerge {
        /// JSON con la colección de cambios
        #[arg(long)]
        changes: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Actualiza un valor dentro del data de un nodo ubicado por path (Patch, :patch_m)
    Patch {
        /// Path del nodo, ej /tenants/t/documentlibrary/x
        envelope_path: String,
        /// Ruta dentro del data (claves separadas)
        #[arg(long, required = true, num_args = 1..)]
        path: Vec<String>,
        /// Valor JSON a colocar
        #[arg(long)]
        data: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Lee un nodo por path, opcionalmente un valor interno del data (Get, :get_m)
    GetIn {
        /// Path del nodo
        node_path: String,
        /// Ruta interna del data (vacío = nodo completo)
        #[arg(long, num_args = 0..)]
        path: Vec<String>,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Busca nodos por nombre (NodeByName, :nodebyname)
    ByName {
        name: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Descarga el contenido binario por gRPC (NodeContent, :nodecontent_m)
    Content {
        id: String,
        /// Archivo de salida (default: <unique_id>.bin)
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Crea un nodo hijo sin contenido (NodeCreate, :node_m)
    Create {
        #[arg(long)]
        parent: String,
        #[arg(long = "type")]
        node_type: String,
        /// JSON del data; DEBE incluir "name"
        #[arg(long)]
        data: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Agrega un secondary parent (AddSecondaryParent, :addsecundaryparent)
    AddSecondary {
        child_id: String,
        parent_id: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Quita un secondary parent (RemoveSecondaryParent, :removesecundaryparent)
    RemoveSecondary {
        child_id: String,
        parent_id: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
}

#[derive(Subcommand)]
enum TenantCmd {
    /// Obtiene el nodo del tenant (TenantGet, :tenant)
    Get {
        tenant: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Crea un tenant (TenantCreate, :tenant)
    Create {
        tenant: String,
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "")]
        description: String,
        #[arg(long)]
        dni: String,
        #[arg(long)]
        company: String,
        #[arg(long)]
        email: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Document library del tenant (DocLib, :doc_lib)
    Doclib {
        tenant: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Home de un tipo documental (Home, :home)
    Home {
        tenant: String,
        #[arg(long = "type")]
        node_type: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Package/home clasificado de un tipo (Package, :package_m)
    Package {
        tenant: String,
        #[arg(long = "type")]
        node_type: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
}

#[derive(Subcommand)]
enum AdminCmd {
    /// Crea un folder bajo un nodo (Folder, :folder)
    Folder {
        #[arg(long)]
        parent: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "")]
        description: String,
        #[arg(long, default_value = "{}")]
        data: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Crea el grupo default con nombre (DefaultGroup, :default_group)
    DefaultGroup {
        #[arg(long)]
        name: String,
        #[arg(long)]
        parent: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Grupo colaborador default del tenant (:default_colaborator_group)
    ColaboratorGroup {
        #[arg(long)]
        parent: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Grupo consumidor default del tenant (:default_consumer_group)
    ConsumerGroup {
        #[arg(long)]
        parent: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Grupo administrador default del tenant (:default_administrator_group)
    AdministratorGroup {
        #[arg(long)]
        parent: String,
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Índices configurados en etcd (Indexs, :indexs)
    Indexs {
        #[command(flatten)]
        grpc: GrpcOpts,
    },
    /// Tipos documentales del document library (DocLibsTypes, :doc_libs_types)
    DoclibTypes {
        #[command(flatten)]
        grpc: GrpcOpts,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Cmd::Upload {
            file,
            node_type,
            title,
            description,
            parent,
            user,
            tenant,
            data,
            assoc,
            signed_ref,
            endpoint,
            api_key,
            retries,
        } => {
            if assoc.is_some() && signed_ref.is_some() {
                bail!("--assoc y --signed-ref son mutuamente excluyentes");
            }
            // validar JSON de --data antes de enviar
            serde_json::from_str::<serde_json::Value>(&data)
                .context("--data no es JSON valido")?;

            let filename = file
                .file_name()
                .context("ruta de archivo invalida")?
                .to_string_lossy()
                .to_string();

            let variant = if assoc.is_some() {
                "assoc"
            } else if signed_ref.is_some() {
                "signed"
            } else {
                "plain"
            };

            let meta = TransferMeta {
                tenant,
                r#type: node_type,
                title: title.unwrap_or_else(|| filename.clone()),
                description,
                filename,
                parent_id: parent,
                username: user,
                data_json: data,
                variant: variant.into(),
                assoc_id: assoc.unwrap_or_default(),
                ref_signed_id: signed_ref.unwrap_or_default(),
                // Idempotencia: UNA clave por invocación, compartida por todos
                // los reintentos — un retry jamás duplica el documento.
                client_ref: uuid::Uuid::new_v4().to_string(),
            };

            upload_with_retries(&endpoint, &file, meta, &api_key, retries).await?;
        }

        Cmd::Node { cmd } => match cmd {
            NodeCmd::Get { id, grpc } => {
                let req = nodemanager::UniqueIdRequest { unique_id: id };
                let mut c = nm_client(&grpc).await?;
                print_monadic(c.node_get(with_key(req, &grpc)?).await?.into_inner())?;
            }
            NodeCmd::Ids { ids, node_type, grpc } => {
                let req = nodemanager::IdsRequest { ids, r#type: node_type };
                let mut c = nm_client(&grpc).await?;
                print_monadic(c.ids(with_key(req, &grpc)?).await?.into_inner())?;
            }
            NodeCmd::ByType { node_type, tenant, grpc } => {
                let req = nodemanager::HomeRequest { tenant, r#type: node_type };
                let mut c = nm_client(&grpc).await?;
                print_monadic(c.by_type(with_key(req, &grpc)?).await?.into_inner())?;
            }
            NodeCmd::ByPath { path, tenant, grpc } => {
                let req = nodemanager::ByPathRequest { tenant, path };
                let mut c = nm_client(&grpc).await?;
                print_monadic(c.by_path(with_key(req, &grpc)?).await?.into_inner())?;
            }
            NodeCmd::Children { id, secondary, grpc } => {
                let req = nodemanager::NodeChildRequest { unique_id: id, secondary };
                let mut c = nm_client(&grpc).await?;
                print_monadic(c.node_child(with_key(req, &grpc)?).await?.into_inner())?;
            }
            NodeCmd::User { username, grpc } => {
                let req = nodemanager::UserRequest { username, password: String::new() };
                let mut c = nm_client(&grpc).await?;
                print_monadic(c.user(with_key(req, &grpc)?).await?.into_inner())?;
            }
            NodeCmd::Datamerge { id, data, grpc } => {
                serde_json::from_str::<serde_json::Value>(&data)
                    .context("--data no es JSON valido")?;
                let req = nodemanager::DatamergeRequest { unique_id: id, data_json: data };
                let mut c = nm_client(&grpc).await?;
                print_monadic(c.datamerge(with_key(req, &grpc)?).await?.into_inner())?;
            }
            NodeCmd::DataUpdate { id, data, grpc } => {
                serde_json::from_str::<serde_json::Value>(&data)
                    .context("--data no es JSON valido")?;
                let req = nodemanager::DataUpdateRequest { unique_id: id, data_json: data };
                let mut c = nm_client(&grpc).await?;
                print_monadic(c.data_update(with_key(req, &grpc)?).await?.into_inner())?;
            }
            NodeCmd::BulkDatamerge { changes, grpc } => {
                serde_json::from_str::<serde_json::Value>(&changes)
                    .context("--changes no es JSON valido")?;
                let req = nodemanager::BulkDatamergeRequest { changes_json: changes };
                let mut c = nm_client(&grpc).await?;
                print_monadic(c.bulk_datamerge(with_key(req, &grpc)?).await?.into_inner())?;
            }
            NodeCmd::Patch { envelope_path, path, data, grpc } => {
                serde_json::from_str::<serde_json::Value>(&data)
                    .context("--data no es JSON valido")?;
                let req = nodemanager::PatchRequest { envelope_path, path, data_json: data };
                let mut c = nm_client(&grpc).await?;
                print_monadic(c.patch(with_key(req, &grpc)?).await?.into_inner())?;
            }
            NodeCmd::GetIn { node_path, path, grpc } => {
                let req = nodemanager::GetRequest { node_path, path };
                let mut c = nm_client(&grpc).await?;
                print_monadic(c.get(with_key(req, &grpc)?).await?.into_inner())?;
            }
            NodeCmd::ByName { name, grpc } => {
                let req = nodemanager::NameRequest { name };
                let mut c = nm_client(&grpc).await?;
                print_monadic(c.node_by_name(with_key(req, &grpc)?).await?.into_inner())?;
            }
            NodeCmd::Content { id, output, grpc } => {
                let out = output.unwrap_or_else(|| PathBuf::from(format!("{id}.bin")));
                let req = nodemanager::UniqueIdRequest { unique_id: id };
                let mut c = nm_client(&grpc).await?;
                let reply = c.node_content(with_key(req, &grpc)?).await?.into_inner();
                if reply.ok {
                    tokio::fs::write(&out, &reply.content).await?;
                    eprintln!("descargado: {} ({} bytes)", out.display(), reply.content.len());
                } else {
                    bail!("{{:error, {}}}", reply.error);
                }
            }
            NodeCmd::Create { parent, node_type, data, grpc } => {
                serde_json::from_str::<serde_json::Value>(&data)
                    .context("--data no es JSON valido")?;
                let req = nodemanager::NodeCreateRequest {
                    parent_id: parent,
                    data_json: data,
                    r#type: node_type,
                };
                let mut c = nm_client(&grpc).await?;
                print_monadic(c.node_create(with_key(req, &grpc)?).await?.into_inner())?;
            }
            NodeCmd::AddSecondary { child_id, parent_id, grpc } => {
                let req = nodemanager::SecondaryParentRequest { child_id, parent_id };
                let mut c = nm_client(&grpc).await?;
                print_monadic(c.add_secondary_parent(with_key(req, &grpc)?).await?.into_inner())?;
            }
            NodeCmd::RemoveSecondary { child_id, parent_id, grpc } => {
                let req = nodemanager::SecondaryParentRequest { child_id, parent_id };
                let mut c = nm_client(&grpc).await?;
                print_monadic(c.remove_secondary_parent(with_key(req, &grpc)?).await?.into_inner())?;
            }
        },

        Cmd::Tenant { cmd } => match cmd {
            TenantCmd::Get { tenant, grpc } => {
                let req = nodemanager::TenantGetRequest { tenant };
                let mut c = nm_client(&grpc).await?;
                print_monadic(c.tenant_get(with_key(req, &grpc)?).await?.into_inner())?;
            }
            TenantCmd::Create { tenant, title, description, dni, company, email, grpc } => {
                let req = nodemanager::TenantCreateRequest {
                    tenant,
                    title,
                    description,
                    dni,
                    company_name: company,
                    email,
                };
                let mut c = nm_client(&grpc).await?;
                print_monadic(c.tenant_create(with_key(req, &grpc)?).await?.into_inner())?;
            }
            TenantCmd::Doclib { tenant, grpc } => {
                let req = nodemanager::TenantRequest { tenant };
                let mut c = nm_client(&grpc).await?;
                print_monadic(c.doc_lib(with_key(req, &grpc)?).await?.into_inner())?;
            }
            TenantCmd::Home { tenant, node_type, grpc } => {
                let req = nodemanager::HomeRequest { tenant, r#type: node_type };
                let mut c = nm_client(&grpc).await?;
                print_monadic(c.home(with_key(req, &grpc)?).await?.into_inner())?;
            }
            TenantCmd::Package { tenant, node_type, grpc } => {
                let req = nodemanager::PackageRequest { tenant, r#type: node_type };
                let mut c = nm_client(&grpc).await?;
                print_monadic(c.package(with_key(req, &grpc)?).await?.into_inner())?;
            }
        },

        Cmd::Admin { cmd } => match cmd {
            AdminCmd::Folder { parent, name, title, description, data, grpc } => {
                serde_json::from_str::<serde_json::Value>(&data)
                    .context("--data no es JSON valido")?;
                let req = nodemanager::FolderRequest {
                    parent_id: parent,
                    data_json: data,
                    name,
                    title,
                    description,
                };
                let mut c = nm_client(&grpc).await?;
                print_monadic(c.folder(with_key(req, &grpc)?).await?.into_inner())?;
            }
            AdminCmd::DefaultGroup { name, parent, grpc } => {
                let req = nodemanager::DefaultGroupRequest { name, parent_id: parent };
                let mut c = nm_client(&grpc).await?;
                print_monadic(c.default_group(with_key(req, &grpc)?).await?.into_inner())?;
            }
            AdminCmd::ColaboratorGroup { parent, grpc } => {
                let req = nodemanager::ParentRequest { parent_id: parent };
                let mut c = nm_client(&grpc).await?;
                print_monadic(
                    c.default_colaborator_group(with_key(req, &grpc)?).await?.into_inner(),
                )?;
            }
            AdminCmd::ConsumerGroup { parent, grpc } => {
                let req = nodemanager::ParentRequest { parent_id: parent };
                let mut c = nm_client(&grpc).await?;
                print_monadic(c.default_consumer_group(with_key(req, &grpc)?).await?.into_inner())?;
            }
            AdminCmd::AdministratorGroup { parent, grpc } => {
                let req = nodemanager::ParentRequest { parent_id: parent };
                let mut c = nm_client(&grpc).await?;
                print_monadic(
                    c.default_administrator_group(with_key(req, &grpc)?).await?.into_inner(),
                )?;
            }
            AdminCmd::Indexs { grpc } => {
                let req = nodemanager::EmptyRequest {};
                let mut c = nm_client(&grpc).await?;
                print_monadic(c.indexs(with_key(req, &grpc)?).await?.into_inner())?;
            }
            AdminCmd::DoclibTypes { grpc } => {
                let req = nodemanager::EmptyRequest {};
                let mut c = nm_client(&grpc).await?;
                print_monadic(c.doc_libs_types(with_key(req, &grpc)?).await?.into_inner())?;
            }
        },

        Cmd::Download { id, tenant, output, rest_url, api_key } => {
            let url = format!("{rest_url}/nodeservice/download/tenant/{tenant}/node/{id}");
            let resp = reqwest::Client::new()
                .get(&url)
                .header("x-api-key", &api_key)
                .send()
                .await
                .context("fallo la peticion REST")?;
            if !resp.status().is_success() {
                bail!("HTTP {}: {}", resp.status(), resp.text().await.unwrap_or_default());
            }
            let out = output.unwrap_or_else(|| PathBuf::from(format!("{id}.bin")));
            let bytes = resp.bytes().await?;
            tokio::fs::write(&out, &bytes).await?;
            eprintln!("descargado: {} ({} bytes)", out.display(), bytes.len());
        }
    }

    Ok(())
}

// --- upload gRPC -------------------------------------------------------------

async fn upload_with_retries(
    endpoint: &str,
    file: &PathBuf,
    meta: TransferMeta,
    api_key: &str,
    retries: u32,
) -> Result<()> {
    let mut last_err = None;

    for attempt in 1..=retries {
        match upload_once(endpoint, file, meta.clone(), api_key).await {
            Ok(reply) => {
                println!(
                    "{}",
                    serde_json::json!({
                        "unique_id": reply.unique_id,
                        "transfer_id": reply.transfer_id,
                        "status": reply.status,
                        "duplicated": reply.duplicated,
                    })
                );
                if reply.status == "completed" {
                    return Ok(());
                }
                // error de negocio (ids inexistentes, etc.): permanente
                bail!("upload fallo: {} ({})", reply.status, reply.result);
            }
            Err(e) => {
                eprintln!("intento {attempt}/{retries} fallo: {e:#}");
                last_err = Some(e);
                if attempt < retries {
                    tokio::time::sleep(Duration::from_secs(attempt as u64)).await;
                }
            }
        }
    }

    Err(last_err.unwrap()).context("se agotaron los reintentos")
}

async fn upload_once(
    endpoint: &str,
    file: &PathBuf,
    meta: TransferMeta,
    api_key: &str,
) -> Result<transfer::TransferReply> {
    let size = tokio::fs::metadata(file).await?.len();
    let mut fh = tokio::fs::File::open(file).await?;

    let pb = ProgressBar::new(size);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{bar:38.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec})",
        )?
        .progress_chars("#>-"),
    );

    let pb2 = pb.clone();
    let outbound = async_stream::stream! {
        // 1º SIEMPRE la metadata
        yield ChunkRequest { payload: Some(Payload::Meta(meta)) };

        // luego el contenido en trozos de 64 KiB
        let mut buf = vec![0u8; CHUNK_SIZE];
        loop {
            match fh.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    pb2.inc(n as u64);
                    yield ChunkRequest { payload: Some(Payload::Chunk(buf[..n].to_vec())) };
                }
                Err(_) => break,
            }
        }
    };

    let channel = tonic::transport::Channel::from_shared(endpoint.to_string())
        .context("endpoint invalido (usa http://host:puerto)")?
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(360))
        .connect()
        .await
        .context("no se pudo conectar al endpoint gRPC")?;

    let mut client = BinaryTransferServiceClient::new(channel)
        .max_encoding_message_size(1024 * 1024)
        .max_decoding_message_size(1024 * 1024);

    // API key en metadata gRPC (header x-api-key), igual que la capa HTTP
    let mut request = tonic::Request::new(outbound);
    request.metadata_mut().insert(
        "x-api-key",
        api_key.parse().context("api key con caracteres invalidos")?,
    );

    let reply = client.upload(request).await?.into_inner();
    pb.finish_and_clear();
    Ok(reply)
}

// --- NodeManagerService (gRPC) -----------------------------------------------

async fn nm_client(
    grpc: &GrpcOpts,
) -> Result<NodeManagerServiceClient<tonic::transport::Channel>> {
    let channel = tonic::transport::Channel::from_shared(grpc.endpoint.clone())
        .context("endpoint invalido (usa http://host:puerto)")?
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .connect()
        .await
        .context("no se pudo conectar al endpoint gRPC")?;

    Ok(NodeManagerServiceClient::new(channel))
}

fn with_key<T>(req: T, grpc: &GrpcOpts) -> Result<tonic::Request<T>> {
    let mut request = tonic::Request::new(req);
    request.metadata_mut().insert(
        "x-api-key",
        grpc.api_key.parse().context("api key con caracteres invalidos")?,
    );
    Ok(request)
}

/// Imprime la respuesta monádica: ok=true -> result_json a stdout;
/// ok=false -> error a stderr y exit != 0 ({:error, _}).
fn print_monadic(reply: nodemanager::MonadicReply) -> Result<()> {
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

// --- REST helpers ------------------------------------------------------------

async fn rest_json(url: &str, api_key: &str) -> Result<()> {
    let resp = reqwest::Client::new()
        .get(url)
        .header("x-api-key", api_key)
        .send()
        .await
        .context("fallo la peticion REST")?;
    let status = resp.status();
    let body = resp.text().await?;

    match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(v) => println!("{}", serde_json::to_string_pretty(&v)?),
        Err(_) => println!("{body}"),
    }

    if !status.is_success() {
        bail!("HTTP {status}");
    }
    Ok(())
}
