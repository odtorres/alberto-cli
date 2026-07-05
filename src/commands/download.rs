//! `alberto download` — contenido binario de un nodo (NodeContent).

use std::path::PathBuf;

use anyhow::{bail, Result};

use crate::cli::GrpcOpts;
use crate::client::{nm_client, nodemanager as nm, with_key};

pub async fn run(id: String, dest: Option<PathBuf>, grpc: GrpcOpts) -> Result<()> {
    let out = dest.unwrap_or_else(|| PathBuf::from(format!("{id}.bin")));
    let req = nm::UniqueIdRequest { unique_id: id };
    let (mut c, conn) = nm_client(&grpc).await?;
    let reply = c
        .node_content(with_key(req, &conn.api_key)?)
        .await?
        .into_inner();
    if reply.ok {
        tokio::fs::write(&out, &reply.content).await?;
        eprintln!(
            "descargado: {} ({} bytes)",
            out.display(),
            reply.content.len()
        );
        Ok(())
    } else {
        bail!("{{:error, {}}}", reply.error);
    }
}
