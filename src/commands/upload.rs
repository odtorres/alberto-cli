//! `alberto upload` — gRPC client-streaming con idempotencia y reintentos.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use tokio::io::AsyncReadExt;

use super::valid_json;
use crate::cli::UploadArgs;
use crate::client::transfer;
use crate::client::transfer::binary_transfer_service_client::BinaryTransferServiceClient;
use crate::client::transfer::{chunk_request::Payload, ChunkRequest, TransferMeta};

const CHUNK_SIZE: usize = 64 * 1024;

pub async fn run(a: UploadArgs) -> Result<()> {
    if a.assoc.is_some() && a.signed_ref.is_some() {
        bail!("--assoc y --signed-ref son mutuamente excluyentes");
    }
    valid_json(&a.data, "--data")?;

    let conn = a.grpc.resolve()?;

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

    upload_with_retries(&conn.endpoint, &a.file, meta, &conn.api_key, a.retries).await
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
        api_key
            .parse()
            .context("api key con caracteres invalidos")?,
    );

    let reply = client.upload(request).await?.into_inner();
    pb.finish_and_clear();
    Ok(reply)
}
