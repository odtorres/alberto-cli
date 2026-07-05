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
    request.metadata_mut().insert(
        "x-api-key",
        api_key
            .parse()
            .context("api key con caracteres invalidos")?,
    );
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
