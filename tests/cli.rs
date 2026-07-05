//! End-to-end: el binario `alberto` contra un NodeService gRPC de mentira.

use std::net::SocketAddr;

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;
use tokio::net::TcpListener;
use tonic::{Request, Response, Status};

use alberto_cli::client::nodemanager::node_manager_service_server::{
    NodeManagerService, NodeManagerServiceServer,
};
use alberto_cli::client::nodemanager::*;
use alberto_cli::client::transfer::binary_transfer_service_server::{
    BinaryTransferService, BinaryTransferServiceServer,
};
use alberto_cli::client::transfer::{chunk_request, ChunkRequest, TransferReply};

const KEY: &str = "test-key";

// Result<_, Status> es intencional: refleja la firma real de las RPCs
// mockeadas; Status ya sale "grande" en el trait generado por tonic-build.
#[allow(clippy::result_large_err)]
fn auth<T>(req: &Request<T>) -> Result<(), Status> {
    match req.metadata().get("x-api-key") {
        Some(v) if v == KEY => Ok(()),
        _ => Err(Status::unauthenticated("x-api-key invalida")),
    }
}

#[allow(clippy::result_large_err)]
fn ok_reply(json: &str) -> Result<Response<MonadicReply>, Status> {
    Ok(Response::new(MonadicReply {
        ok: true,
        result_json: json.into(),
        error: String::new(),
        content: vec![],
    }))
}

#[derive(Default)]
struct MockNm;

#[tonic::async_trait]
impl NodeManagerService for MockNm {
    async fn node_get(
        &self,
        r: Request<UniqueIdRequest>,
    ) -> Result<Response<MonadicReply>, Status> {
        auth(&r)?;
        let id = r.into_inner().unique_id;
        ok_reply(&format!(
            r#"{{"unique_id":"{id}","name":"doc.pdf","content":true}}"#
        ))
    }

    async fn tenant_get(
        &self,
        r: Request<TenantGetRequest>,
    ) -> Result<Response<MonadicReply>, Status> {
        auth(&r)?;
        let t = r.into_inner().tenant;
        ok_reply(&format!(r#"{{"tenant":"{t}"}}"#))
    }

    async fn indexs(&self, r: Request<EmptyRequest>) -> Result<Response<MonadicReply>, Status> {
        auth(&r)?;
        ok_reply(r#"["rut","folio"]"#)
    }

    // error de negocio: {:error, not_found}
    async fn user(&self, r: Request<UserRequest>) -> Result<Response<MonadicReply>, Status> {
        auth(&r)?;
        Ok(Response::new(MonadicReply {
            ok: false,
            result_json: String::new(),
            error: "not_found".into(),
            content: vec![],
        }))
    }

    async fn node_content(
        &self,
        r: Request<UniqueIdRequest>,
    ) -> Result<Response<MonadicReply>, Status> {
        auth(&r)?;
        Ok(Response::new(MonadicReply {
            ok: true,
            result_json: String::new(),
            error: String::new(),
            content: b"%PDF-fake-content".to_vec(),
        }))
    }

    // Nota: no se puede generar estos métodos con una macro_rules! invocada
    // dentro de este impl: #[tonic::async_trait] (async-trait) transforma el
    // `async fn` que ve literalmente en el árbol de sintaxis del impl antes de
    // que la invocación anidada de la macro se expanda; el resultado sería un
    // `async fn` plano que no calza con la firma (Pin<Box<dyn Future>>) que
    // exige el trait. Por eso se escriben explícitos en vez de vía macro.
    async fn ids(&self, _r: Request<IdsRequest>) -> Result<Response<MonadicReply>, Status> {
        Err(Status::unimplemented("ids"))
    }
    async fn data_update(
        &self,
        _r: Request<DataUpdateRequest>,
    ) -> Result<Response<MonadicReply>, Status> {
        Err(Status::unimplemented("data_update"))
    }
    async fn bulk_datamerge(
        &self,
        _r: Request<BulkDatamergeRequest>,
    ) -> Result<Response<MonadicReply>, Status> {
        Err(Status::unimplemented("bulk_datamerge"))
    }
    async fn patch(&self, _r: Request<PatchRequest>) -> Result<Response<MonadicReply>, Status> {
        Err(Status::unimplemented("patch"))
    }
    async fn get(&self, _r: Request<GetRequest>) -> Result<Response<MonadicReply>, Status> {
        Err(Status::unimplemented("get"))
    }
    async fn datamerge(
        &self,
        _r: Request<DatamergeRequest>,
    ) -> Result<Response<MonadicReply>, Status> {
        Err(Status::unimplemented("datamerge"))
    }
    async fn node_by_name(
        &self,
        _r: Request<NameRequest>,
    ) -> Result<Response<MonadicReply>, Status> {
        Err(Status::unimplemented("node_by_name"))
    }
    async fn package(&self, _r: Request<PackageRequest>) -> Result<Response<MonadicReply>, Status> {
        Err(Status::unimplemented("package"))
    }
    async fn node_create(
        &self,
        _r: Request<NodeCreateRequest>,
    ) -> Result<Response<MonadicReply>, Status> {
        Err(Status::unimplemented("node_create"))
    }
    async fn tenant_create(
        &self,
        _r: Request<TenantCreateRequest>,
    ) -> Result<Response<MonadicReply>, Status> {
        Err(Status::unimplemented("tenant_create"))
    }
    async fn folder(&self, _r: Request<FolderRequest>) -> Result<Response<MonadicReply>, Status> {
        Err(Status::unimplemented("folder"))
    }
    async fn default_group(
        &self,
        _r: Request<DefaultGroupRequest>,
    ) -> Result<Response<MonadicReply>, Status> {
        Err(Status::unimplemented("default_group"))
    }
    async fn default_colaborator_group(
        &self,
        _r: Request<ParentRequest>,
    ) -> Result<Response<MonadicReply>, Status> {
        Err(Status::unimplemented("default_colaborator_group"))
    }
    async fn default_consumer_group(
        &self,
        _r: Request<ParentRequest>,
    ) -> Result<Response<MonadicReply>, Status> {
        Err(Status::unimplemented("default_consumer_group"))
    }
    async fn default_administrator_group(
        &self,
        _r: Request<ParentRequest>,
    ) -> Result<Response<MonadicReply>, Status> {
        Err(Status::unimplemented("default_administrator_group"))
    }
    async fn doc_lib(&self, _r: Request<TenantRequest>) -> Result<Response<MonadicReply>, Status> {
        Err(Status::unimplemented("doc_lib"))
    }
    async fn home(&self, _r: Request<HomeRequest>) -> Result<Response<MonadicReply>, Status> {
        Err(Status::unimplemented("home"))
    }
    async fn doc_libs_types(
        &self,
        _r: Request<EmptyRequest>,
    ) -> Result<Response<MonadicReply>, Status> {
        Err(Status::unimplemented("doc_libs_types"))
    }
    async fn by_type(&self, _r: Request<HomeRequest>) -> Result<Response<MonadicReply>, Status> {
        Err(Status::unimplemented("by_type"))
    }
    async fn by_path(&self, _r: Request<ByPathRequest>) -> Result<Response<MonadicReply>, Status> {
        Err(Status::unimplemented("by_path"))
    }
    async fn node_child(
        &self,
        _r: Request<NodeChildRequest>,
    ) -> Result<Response<MonadicReply>, Status> {
        Err(Status::unimplemented("node_child"))
    }
    async fn add_secondary_parent(
        &self,
        _r: Request<SecondaryParentRequest>,
    ) -> Result<Response<MonadicReply>, Status> {
        Err(Status::unimplemented("add_secondary_parent"))
    }
    async fn remove_secondary_parent(
        &self,
        _r: Request<SecondaryParentRequest>,
    ) -> Result<Response<MonadicReply>, Status> {
        Err(Status::unimplemented("remove_secondary_parent"))
    }
}

#[derive(Default)]
struct MockTransfer;

#[tonic::async_trait]
impl BinaryTransferService for MockTransfer {
    async fn upload(
        &self,
        r: Request<tonic::Streaming<ChunkRequest>>,
    ) -> Result<Response<TransferReply>, Status> {
        auth(&r)?;
        let mut stream = r.into_inner();
        let mut bytes = 0usize;
        while let Some(msg) = stream.message().await? {
            if let Some(chunk_request::Payload::Chunk(c)) = msg.payload {
                bytes += c.len();
            }
        }
        Ok(Response::new(TransferReply {
            transfer_id: "t-1".into(),
            unique_id: "u-1".into(),
            status: "completed".into(),
            result: format!("{bytes} bytes"),
            duplicated: false,
        }))
    }
}

async fn spawn_mock() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(NodeManagerServiceServer::new(MockNm))
            .add_service(BinaryTransferServiceServer::new(MockTransfer))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    addr
}

/// Comando limpio: sin env vars del usuario que cambien endpoint/key/perfil.
fn alberto() -> Command {
    let mut cmd = Command::cargo_bin("alberto").unwrap();
    for var in [
        "ALBERTO_GRPC_ENDPOINT",
        "ALBERTO_REST_URL",
        "ALBERTO_API_KEY",
        "ALBERTO_PROFILE",
        "ALBERTO_CONFIG",
    ] {
        cmd.env_remove(var);
    }
    cmd.env("ALBERTO_CONFIG", "/nonexistent/alberto-config.toml");
    cmd
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn node_get_happy() {
    let addr = spawn_mock().await;
    alberto()
        .args([
            "node",
            "get",
            "abc",
            "--endpoint",
            &format!("http://{addr}"),
            "--api-key",
            KEY,
        ])
        .assert()
        .success()
        .stdout(contains(r#""unique_id": "abc""#));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn node_get_output_raw() {
    let addr = spawn_mock().await;
    alberto()
        .args([
            "node",
            "get",
            "abc",
            "--endpoint",
            &format!("http://{addr}"),
            "--api-key",
            KEY,
            "--output",
            "raw",
        ])
        .assert()
        .success()
        .stdout(contains(
            r#"{"unique_id":"abc","name":"doc.pdf","content":true}"#,
        ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn node_user_business_error() {
    let addr = spawn_mock().await;
    alberto()
        .args([
            "node",
            "user",
            "nadie",
            "--endpoint",
            &format!("http://{addr}"),
            "--api-key",
            KEY,
        ])
        .assert()
        .failure()
        .stderr(contains("{:error, not_found}"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bad_api_key_is_unauthenticated() {
    let addr = spawn_mock().await;
    alberto()
        .args([
            "node",
            "get",
            "abc",
            "--endpoint",
            &format!("http://{addr}"),
            "--api-key",
            "mala",
        ])
        .assert()
        .failure()
        .stderr(contains("x-api-key invalida"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tenant_get_happy() {
    let addr = spawn_mock().await;
    alberto()
        .args([
            "tenant",
            "get",
            "acme",
            "--endpoint",
            &format!("http://{addr}"),
            "--api-key",
            KEY,
        ])
        .assert()
        .success()
        .stdout(contains(r#""tenant": "acme""#));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn admin_indexs_happy() {
    let addr = spawn_mock().await;
    alberto()
        .args([
            "admin",
            "indexs",
            "--endpoint",
            &format!("http://{addr}"),
            "--api-key",
            KEY,
        ])
        .assert()
        .success()
        .stdout(contains("rut"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn download_writes_file() {
    let addr = spawn_mock().await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("out.pdf");
    alberto()
        .args([
            "download",
            "abc",
            dest.to_str().unwrap(),
            "--endpoint",
            &format!("http://{addr}"),
            "--api-key",
            KEY,
        ])
        .assert()
        .success();
    assert_eq!(std::fs::read(&dest).unwrap(), b"%PDF-fake-content");
}

/// Camino de error por familia: la auth compartida rechaza en todas.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bad_api_key_fails_every_family() {
    let addr = spawn_mock().await;
    let ep = format!("http://{addr}");
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("f.txt");
    std::fs::write(&file, b"x").unwrap();

    let cases: Vec<Vec<&str>> = vec![
        vec!["tenant", "get", "acme"],
        vec!["admin", "indexs"],
        vec!["download", "abc"],
        vec![
            "upload",
            file.to_str().unwrap(),
            "--type",
            "t",
            "--parent",
            "p",
            "--user",
            "u",
            "--retries",
            "1",
        ],
    ];
    for args in cases {
        alberto()
            .args(&args)
            .args(["--endpoint", &ep, "--api-key", "mala"])
            .assert()
            .failure();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn upload_happy() {
    let addr = spawn_mock().await;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("f.txt");
    std::fs::write(&file, b"hola mundo").unwrap();
    alberto()
        .args([
            "upload",
            file.to_str().unwrap(),
            "--type",
            "factura",
            "--parent",
            "p-1",
            "--user",
            "oscar",
            "--endpoint",
            &format!("http://{addr}"),
            "--api-key",
            KEY,
        ])
        .assert()
        .success()
        .stdout(contains("completed"));
}

#[test]
fn config_init_crea_archivo_y_list_lo_muestra() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("config.toml");

    let mut cmd = Command::cargo_bin("alberto").unwrap();
    cmd.env("ALBERTO_CONFIG", &cfg)
        .args(["config", "init"])
        .assert()
        .success();
    assert!(cfg.exists());

    let mut cmd = Command::cargo_bin("alberto").unwrap();
    cmd.env("ALBERTO_CONFIG", &cfg)
        .args(["config", "list"])
        .assert()
        .success()
        .stdout(contains("local"));
}

#[test]
fn config_show_enmascara_api_key() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("config.toml");
    std::fs::write(
        &cfg,
        "default_profile = \"qa\"\n[profiles.qa]\nendpoint = \"http://qa:1\"\napi_key = \"supersecreta\"\n",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("alberto").unwrap();
    cmd.env("ALBERTO_CONFIG", &cfg)
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(contains("supe…").and(contains("supersecreta").not()));
}
