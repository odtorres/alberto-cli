//! alberto — CLI para NodeService.
//!
//! * `upload`   → gRPC client-streaming (feature upload_by_streaming_with_backpressure)
//!   con idempotencia (client_ref) y reintentos automáticos.
//! * `node`     → operaciones REST contra node_manager (get / children).
//! * `download` → descarga el contenido binario de un nodo vía REST.
//!
//! Endpoints por defecto (pensados para `kubectl port-forward`):
//!   gRPC: http://127.0.0.1:9090   (override: --endpoint o ALBERTO_GRPC_ENDPOINT)
//!   REST: http://127.0.0.1:3537  (override: --rest-url o ALBERTO_REST_URL)

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use tokio::io::AsyncReadExt;

use alberto_cli::cli::{self, Cli};
use alberto_cli::client::{self, nodemanager, transfer};
use alberto_cli::tui;

use transfer::binary_transfer_service_client::BinaryTransferServiceClient;
use transfer::{chunk_request::Payload, ChunkRequest, TransferMeta};

const CHUNK_SIZE: usize = 64 * 1024;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        cli::Cmd::Upload(a) => {
            if a.assoc.is_some() && a.signed_ref.is_some() {
                bail!("--assoc y --signed-ref son mutuamente excluyentes");
            }
            // validar JSON de --data antes de enviar
            serde_json::from_str::<serde_json::Value>(&a.data)
                .context("--data no es JSON valido")?;

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

            upload_with_retries(&a.endpoint, &a.file, meta, &a.api_key, a.retries).await?;
        }

        cli::Cmd::Node { cmd } => match cmd {
            cli::NodeCmd::Get { id, grpc } => {
                let req = nodemanager::UniqueIdRequest { unique_id: id };
                let mut c = client::nm_client(&grpc).await?;
                client::print_monadic(
                    c.node_get(client::with_key(req, &grpc.api_key)?)
                        .await?
                        .into_inner(),
                )?;
            }
            cli::NodeCmd::Ids {
                ids,
                node_type,
                grpc,
            } => {
                let req = nodemanager::IdsRequest {
                    ids,
                    r#type: node_type,
                };
                let mut c = client::nm_client(&grpc).await?;
                client::print_monadic(
                    c.ids(client::with_key(req, &grpc.api_key)?)
                        .await?
                        .into_inner(),
                )?;
            }
            cli::NodeCmd::ByType {
                node_type,
                tenant,
                grpc,
            } => {
                let req = nodemanager::HomeRequest {
                    tenant,
                    r#type: node_type,
                };
                let mut c = client::nm_client(&grpc).await?;
                client::print_monadic(
                    c.by_type(client::with_key(req, &grpc.api_key)?)
                        .await?
                        .into_inner(),
                )?;
            }
            cli::NodeCmd::ByPath { path, tenant, grpc } => {
                let req = nodemanager::ByPathRequest { tenant, path };
                let mut c = client::nm_client(&grpc).await?;
                client::print_monadic(
                    c.by_path(client::with_key(req, &grpc.api_key)?)
                        .await?
                        .into_inner(),
                )?;
            }
            cli::NodeCmd::Children {
                id,
                secondary,
                grpc,
            } => {
                let req = nodemanager::NodeChildRequest {
                    unique_id: id,
                    secondary,
                };
                let mut c = client::nm_client(&grpc).await?;
                client::print_monadic(
                    c.node_child(client::with_key(req, &grpc.api_key)?)
                        .await?
                        .into_inner(),
                )?;
            }
            cli::NodeCmd::User { username, grpc } => {
                let req = nodemanager::UserRequest {
                    username,
                    password: String::new(),
                };
                let mut c = client::nm_client(&grpc).await?;
                client::print_monadic(
                    c.user(client::with_key(req, &grpc.api_key)?)
                        .await?
                        .into_inner(),
                )?;
            }
            cli::NodeCmd::Datamerge { id, data, grpc } => {
                serde_json::from_str::<serde_json::Value>(&data)
                    .context("--data no es JSON valido")?;
                let req = nodemanager::DatamergeRequest {
                    unique_id: id,
                    data_json: data,
                };
                let mut c = client::nm_client(&grpc).await?;
                client::print_monadic(
                    c.datamerge(client::with_key(req, &grpc.api_key)?)
                        .await?
                        .into_inner(),
                )?;
            }
            cli::NodeCmd::DataUpdate { id, data, grpc } => {
                serde_json::from_str::<serde_json::Value>(&data)
                    .context("--data no es JSON valido")?;
                let req = nodemanager::DataUpdateRequest {
                    unique_id: id,
                    data_json: data,
                };
                let mut c = client::nm_client(&grpc).await?;
                client::print_monadic(
                    c.data_update(client::with_key(req, &grpc.api_key)?)
                        .await?
                        .into_inner(),
                )?;
            }
            cli::NodeCmd::BulkDatamerge { changes, grpc } => {
                serde_json::from_str::<serde_json::Value>(&changes)
                    .context("--changes no es JSON valido")?;
                let req = nodemanager::BulkDatamergeRequest {
                    changes_json: changes,
                };
                let mut c = client::nm_client(&grpc).await?;
                client::print_monadic(
                    c.bulk_datamerge(client::with_key(req, &grpc.api_key)?)
                        .await?
                        .into_inner(),
                )?;
            }
            cli::NodeCmd::Patch {
                envelope_path,
                path,
                data,
                grpc,
            } => {
                serde_json::from_str::<serde_json::Value>(&data)
                    .context("--data no es JSON valido")?;
                let req = nodemanager::PatchRequest {
                    envelope_path,
                    path,
                    data_json: data,
                };
                let mut c = client::nm_client(&grpc).await?;
                client::print_monadic(
                    c.patch(client::with_key(req, &grpc.api_key)?)
                        .await?
                        .into_inner(),
                )?;
            }
            cli::NodeCmd::GetIn {
                node_path,
                path,
                grpc,
            } => {
                let req = nodemanager::GetRequest { node_path, path };
                let mut c = client::nm_client(&grpc).await?;
                client::print_monadic(
                    c.get(client::with_key(req, &grpc.api_key)?)
                        .await?
                        .into_inner(),
                )?;
            }
            cli::NodeCmd::ByName { name, grpc } => {
                let req = nodemanager::NameRequest { name };
                let mut c = client::nm_client(&grpc).await?;
                client::print_monadic(
                    c.node_by_name(client::with_key(req, &grpc.api_key)?)
                        .await?
                        .into_inner(),
                )?;
            }
            cli::NodeCmd::Create {
                parent,
                node_type,
                data,
                grpc,
            } => {
                serde_json::from_str::<serde_json::Value>(&data)
                    .context("--data no es JSON valido")?;
                let req = nodemanager::NodeCreateRequest {
                    parent_id: parent,
                    data_json: data,
                    r#type: node_type,
                };
                let mut c = client::nm_client(&grpc).await?;
                client::print_monadic(
                    c.node_create(client::with_key(req, &grpc.api_key)?)
                        .await?
                        .into_inner(),
                )?;
            }
            cli::NodeCmd::AddSecondary {
                child_id,
                parent_id,
                grpc,
            } => {
                let req = nodemanager::SecondaryParentRequest {
                    child_id,
                    parent_id,
                };
                let mut c = client::nm_client(&grpc).await?;
                client::print_monadic(
                    c.add_secondary_parent(client::with_key(req, &grpc.api_key)?)
                        .await?
                        .into_inner(),
                )?;
            }
            cli::NodeCmd::RemoveSecondary {
                child_id,
                parent_id,
                grpc,
            } => {
                let req = nodemanager::SecondaryParentRequest {
                    child_id,
                    parent_id,
                };
                let mut c = client::nm_client(&grpc).await?;
                client::print_monadic(
                    c.remove_secondary_parent(client::with_key(req, &grpc.api_key)?)
                        .await?
                        .into_inner(),
                )?;
            }
        },

        cli::Cmd::Tenant { cmd } => match cmd {
            cli::TenantCmd::Get { tenant, grpc } => {
                let req = nodemanager::TenantGetRequest { tenant };
                let mut c = client::nm_client(&grpc).await?;
                client::print_monadic(
                    c.tenant_get(client::with_key(req, &grpc.api_key)?)
                        .await?
                        .into_inner(),
                )?;
            }
            cli::TenantCmd::Create {
                tenant,
                title,
                description,
                dni,
                company,
                email,
                grpc,
            } => {
                let req = nodemanager::TenantCreateRequest {
                    tenant,
                    title,
                    description,
                    dni,
                    company_name: company,
                    email,
                };
                let mut c = client::nm_client(&grpc).await?;
                client::print_monadic(
                    c.tenant_create(client::with_key(req, &grpc.api_key)?)
                        .await?
                        .into_inner(),
                )?;
            }
            cli::TenantCmd::Doclib { tenant, grpc } => {
                let req = nodemanager::TenantRequest { tenant };
                let mut c = client::nm_client(&grpc).await?;
                client::print_monadic(
                    c.doc_lib(client::with_key(req, &grpc.api_key)?)
                        .await?
                        .into_inner(),
                )?;
            }
            cli::TenantCmd::Home {
                tenant,
                node_type,
                grpc,
            } => {
                let req = nodemanager::HomeRequest {
                    tenant,
                    r#type: node_type,
                };
                let mut c = client::nm_client(&grpc).await?;
                client::print_monadic(
                    c.home(client::with_key(req, &grpc.api_key)?)
                        .await?
                        .into_inner(),
                )?;
            }
            cli::TenantCmd::Package {
                tenant,
                node_type,
                grpc,
            } => {
                let req = nodemanager::PackageRequest {
                    tenant,
                    r#type: node_type,
                };
                let mut c = client::nm_client(&grpc).await?;
                client::print_monadic(
                    c.package(client::with_key(req, &grpc.api_key)?)
                        .await?
                        .into_inner(),
                )?;
            }
        },

        cli::Cmd::Admin { cmd } => match cmd {
            cli::AdminCmd::Folder {
                parent,
                name,
                title,
                description,
                data,
                grpc,
            } => {
                serde_json::from_str::<serde_json::Value>(&data)
                    .context("--data no es JSON valido")?;
                let req = nodemanager::FolderRequest {
                    parent_id: parent,
                    data_json: data,
                    name,
                    title,
                    description,
                };
                let mut c = client::nm_client(&grpc).await?;
                client::print_monadic(
                    c.folder(client::with_key(req, &grpc.api_key)?)
                        .await?
                        .into_inner(),
                )?;
            }
            cli::AdminCmd::DefaultGroup { name, parent, grpc } => {
                let req = nodemanager::DefaultGroupRequest {
                    name,
                    parent_id: parent,
                };
                let mut c = client::nm_client(&grpc).await?;
                client::print_monadic(
                    c.default_group(client::with_key(req, &grpc.api_key)?)
                        .await?
                        .into_inner(),
                )?;
            }
            cli::AdminCmd::ColaboratorGroup { parent, grpc } => {
                let req = nodemanager::ParentRequest { parent_id: parent };
                let mut c = client::nm_client(&grpc).await?;
                client::print_monadic(
                    c.default_colaborator_group(client::with_key(req, &grpc.api_key)?)
                        .await?
                        .into_inner(),
                )?;
            }
            cli::AdminCmd::ConsumerGroup { parent, grpc } => {
                let req = nodemanager::ParentRequest { parent_id: parent };
                let mut c = client::nm_client(&grpc).await?;
                client::print_monadic(
                    c.default_consumer_group(client::with_key(req, &grpc.api_key)?)
                        .await?
                        .into_inner(),
                )?;
            }
            cli::AdminCmd::AdministratorGroup { parent, grpc } => {
                let req = nodemanager::ParentRequest { parent_id: parent };
                let mut c = client::nm_client(&grpc).await?;
                client::print_monadic(
                    c.default_administrator_group(client::with_key(req, &grpc.api_key)?)
                        .await?
                        .into_inner(),
                )?;
            }
            cli::AdminCmd::Indexs { grpc } => {
                let req = nodemanager::EmptyRequest {};
                let mut c = client::nm_client(&grpc).await?;
                client::print_monadic(
                    c.indexs(client::with_key(req, &grpc.api_key)?)
                        .await?
                        .into_inner(),
                )?;
            }
            cli::AdminCmd::DoclibTypes { grpc } => {
                let req = nodemanager::EmptyRequest {};
                let mut c = client::nm_client(&grpc).await?;
                client::print_monadic(
                    c.doc_libs_types(client::with_key(req, &grpc.api_key)?)
                        .await?
                        .into_inner(),
                )?;
            }
        },

        cli::Cmd::Tui { tenant, grpc } => {
            tui::run(tenant, grpc)?;
        }

        cli::Cmd::Download { id, dest, grpc } => {
            let out = dest.unwrap_or_else(|| PathBuf::from(format!("{id}.bin")));
            let req = nodemanager::UniqueIdRequest { unique_id: id };
            let mut c = client::nm_client(&grpc).await?;
            let reply = c
                .node_content(client::with_key(req, &grpc.api_key)?)
                .await?
                .into_inner();
            if reply.ok {
                tokio::fs::write(&out, &reply.content).await?;
                eprintln!(
                    "descargado: {} ({} bytes)",
                    out.display(),
                    reply.content.len()
                );
            } else {
                bail!("{{:error, {}}}", reply.error);
            }
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
        api_key
            .parse()
            .context("api key con caracteres invalidos")?,
    );

    let reply = client.upload(request).await?.into_inner();
    pb.finish_and_clear();
    Ok(reply)
}
