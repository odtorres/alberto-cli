//! `alberto tenant *` — operaciones de tenant.

use anyhow::Result;

use crate::cli::TenantCmd;
use crate::client::{nm_call, nodemanager as nm};

pub async fn run(cmd: TenantCmd) -> Result<()> {
    match cmd {
        TenantCmd::Get { tenant, grpc } => {
            nm_call(
                &grpc,
                nm::TenantGetRequest { tenant },
                |mut c, r| async move { c.tenant_get(r).await },
            )
            .await
        }
        TenantCmd::Create {
            tenant,
            title,
            description,
            dni,
            company,
            email,
            grpc,
        } => {
            nm_call(
                &grpc,
                nm::TenantCreateRequest {
                    tenant,
                    title,
                    description,
                    dni,
                    company_name: company,
                    email,
                },
                |mut c, r| async move { c.tenant_create(r).await },
            )
            .await
        }
        TenantCmd::Doclib { tenant, grpc } => {
            nm_call(&grpc, nm::TenantRequest { tenant }, |mut c, r| async move {
                c.doc_lib(r).await
            })
            .await
        }
        TenantCmd::Home {
            tenant,
            node_type,
            grpc,
        } => {
            nm_call(
                &grpc,
                nm::HomeRequest {
                    tenant,
                    r#type: node_type,
                },
                |mut c, r| async move { c.home(r).await },
            )
            .await
        }
        TenantCmd::Package {
            tenant,
            node_type,
            grpc,
        } => {
            nm_call(
                &grpc,
                nm::PackageRequest {
                    tenant,
                    r#type: node_type,
                },
                |mut c, r| async move { c.package(r).await },
            )
            .await
        }
    }
}
