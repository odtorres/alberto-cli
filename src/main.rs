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
    /// Operaciones de lectura de nodos (REST / node_manager)
    Node {
        #[command(subcommand)]
        cmd: NodeCmd,
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

#[derive(Subcommand)]
enum NodeCmd {
    /// Obtiene un nodo por unique_id (GET /internal/node/:id — requiere API key global)
    Get {
        id: String,
        #[arg(long, env = "ALBERTO_REST_URL", default_value = "http://127.0.0.1:3537")]
        rest_url: String,
        /// API key (header x-api-key)
        #[arg(long, env = "ALBERTO_API_KEY")]
        api_key: String,
    },
    /// Lista nodos por tipo (GET /internal/nodes/type/:type — requiere API key global)
    ByType {
        #[arg(long = "type")]
        node_type: String,
        #[arg(long, env = "ALBERTO_REST_URL", default_value = "http://127.0.0.1:3537")]
        rest_url: String,
        /// API key (header x-api-key)
        #[arg(long, env = "ALBERTO_API_KEY")]
        api_key: String,
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
            NodeCmd::Get { id, rest_url, api_key } => {
                let url = format!("{rest_url}/internal/node/{id}");
                rest_json(&url, &api_key).await?;
            }
            NodeCmd::ByType { node_type, rest_url, api_key } => {
                let url = format!("{rest_url}/internal/nodes/type/{node_type}");
                rest_json(&url, &api_key).await?;
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
