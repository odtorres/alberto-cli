//! Conexión gRPC, autenticación (x-api-key) y presentación de respuestas.

use std::time::Duration;

use anyhow::{bail, Context, Result};

use crate::cli::{Conn, GrpcOpts, Output};
use serde_json::Value;

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
) -> Result<(NodeManagerServiceClient<tonic::transport::Channel>, Conn)> {
    let conn = grpc.resolve()?;
    let channel = tonic::transport::Channel::from_shared(conn.endpoint.clone())
        .context("endpoint invalido (usa http://host:puerto)")?
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .connect()
        .await
        .context("no se pudo conectar al endpoint gRPC")?;

    // NodeContent devuelve el binario completo en un mensaje: subir el límite
    // de decode (default tonic: 4 MB) para archivos grandes.
    Ok((
        NodeManagerServiceClient::new(channel).max_decoding_message_size(1024 * 1024 * 1024),
        conn,
    ))
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

/// Envuelve errores comunes con una pista accionable.
pub fn friendly(e: anyhow::Error) -> anyhow::Error {
    match hint_for(&e) {
        Some(hint) => e.context(hint),
        None => e,
    }
}

fn hint_for(e: &anyhow::Error) -> Option<&'static str> {
    if let Some(status) = e.downcast_ref::<tonic::Status>() {
        return match status.code() {
            tonic::Code::Unauthenticated => {
                Some("pista: revisa el api key (--api-key, ALBERTO_API_KEY o el perfil)")
            }
            tonic::Code::DeadlineExceeded => {
                Some("pista: el servidor no respondió a tiempo — ¿endpoint correcto?")
            }
            _ => None,
        };
    }
    let text = format!("{e:#}");
    if text.contains("no se pudo conectar") || text.contains("Connection refused") {
        return Some(
            "pista: ¿está corriendo el port-forward? (kubectl port-forward svc/nodeservice 9090:9090)",
        );
    }
    None
}

pub fn format_result(result_json: &str, mode: Output) -> String {
    match mode {
        Output::Raw => result_json.to_string(),
        Output::Json => serde_json::from_str::<Value>(result_json)
            .map(|v| v.to_string())
            .unwrap_or_else(|_| result_json.to_string()),
        Output::Pretty => serde_json::from_str::<Value>(result_json)
            .and_then(|v| serde_json::to_string_pretty(&v))
            .unwrap_or_else(|_| result_json.to_string()),
        Output::Table => format_table(result_json),
    }
}

const TABLE_COLS: [&str; 4] = ["unique_id", "name", "type", "content"];

fn format_table(result_json: &str) -> String {
    let Ok(Value::Array(rows)) = serde_json::from_str::<Value>(result_json) else {
        return format_result(result_json, Output::Pretty);
    };

    let cell = |row: &Value, col: &str| match &row[col] {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };

    let mut widths: Vec<usize> = TABLE_COLS.iter().map(|c| c.len()).collect();
    let table: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            TABLE_COLS
                .iter()
                .enumerate()
                .map(|(i, col)| {
                    let v = cell(row, col);
                    widths[i] = widths[i].max(v.len());
                    v
                })
                .collect()
        })
        .collect();

    let fmt_row = |cells: &[String], widths: &[usize]| -> String {
        cells
            .iter()
            .zip(widths.iter().copied())
            .map(|(c, w)| format!("{c:<w$}"))
            .collect::<Vec<_>>()
            .join("  ")
            .trim_end()
            .to_string()
    };

    let header: Vec<String> = TABLE_COLS.iter().map(|s| s.to_string()).collect();
    let mut out = vec![fmt_row(&header, &widths)];
    out.extend(table.iter().map(|r| fmt_row(r, &widths)));
    out.join("\n")
}

/// Ejecuta una RPC monádica: conecta, autentica, llama e imprime.
/// Colapsa el patrón repetido en los ~28 handlers.
pub async fn nm_call<T, F, Fut>(grpc: &GrpcOpts, req: T, call: F) -> Result<()>
where
    F: FnOnce(NodeManagerServiceClient<tonic::transport::Channel>, tonic::Request<T>) -> Fut,
    Fut: std::future::Future<
        Output = std::result::Result<tonic::Response<nodemanager::MonadicReply>, tonic::Status>,
    >,
{
    let (client, conn) = nm_client(grpc).await?;
    let reply = call(client, with_key(req, &conn.api_key)?)
        .await?
        .into_inner();
    print_monadic(reply, grpc.output)
}

/// Imprime la respuesta monádica: ok=true -> result_json a stdout;
/// ok=false -> error a stderr y exit != 0 ({:error, _}).
pub fn print_monadic(reply: nodemanager::MonadicReply, mode: Output) -> Result<()> {
    if reply.ok {
        println!("{}", format_result(&reply.result_json, mode));
        Ok(())
    } else {
        bail!("{{:error, {}}}", reply.error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_pasa_tal_cual() {
        assert_eq!(format_result("{\"a\": 1}", Output::Raw), "{\"a\": 1}");
    }

    #[test]
    fn json_compacta() {
        assert_eq!(format_result("{ \"a\" : 1 }", Output::Json), "{\"a\":1}");
    }

    #[test]
    fn table_lista_columnas() {
        let json = r#"[{"unique_id":"u1","name":"a.pdf","type":"factura","content":true},
                       {"unique_id":"u2","name":"b","type":"folder","content":false}]"#;
        let t = format_result(json, Output::Table);
        let lines: Vec<&str> = t.lines().collect();
        assert!(lines[0].contains("unique_id") && lines[0].contains("name"));
        assert!(lines[1].contains("u1") && lines[1].contains("a.pdf"));
        assert!(lines[2].contains("u2"));
    }

    #[test]
    fn table_no_lista_cae_a_pretty() {
        let t = format_result("{\"a\":1}", Output::Table);
        assert!(t.contains("\"a\": 1"));
    }

    #[test]
    fn hint_para_unauthenticated() {
        let e = anyhow::Error::new(tonic::Status::unauthenticated("x"));
        let msg = format!("{:#}", friendly(e));
        assert!(msg.contains("api key"));
    }

    #[test]
    fn hint_para_connection_refused() {
        let e = anyhow::anyhow!("no se pudo conectar al endpoint gRPC");
        let msg = format!("{:#}", friendly(e));
        assert!(msg.contains("port-forward"));
    }
}
