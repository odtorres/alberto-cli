//! `alberto admin *` — folders, grupos default, índices.

use anyhow::Result;

use super::valid_json;
use crate::cli::AdminCmd;
use crate::client::{nm_call, nodemanager as nm};

pub async fn run(cmd: AdminCmd) -> Result<()> {
    match cmd {
        AdminCmd::Folder {
            parent,
            name,
            title,
            description,
            data,
            grpc,
        } => {
            valid_json(&data, "--data")?;
            nm_call(
                &grpc,
                nm::FolderRequest {
                    parent_id: parent,
                    data_json: data,
                    name,
                    title,
                    description,
                },
                |mut c, r| async move { c.folder(r).await },
            )
            .await
        }
        AdminCmd::DefaultGroup { name, parent, grpc } => {
            nm_call(
                &grpc,
                nm::DefaultGroupRequest {
                    name,
                    parent_id: parent,
                },
                |mut c, r| async move { c.default_group(r).await },
            )
            .await
        }
        AdminCmd::ColaboratorGroup { parent, grpc } => {
            nm_call(
                &grpc,
                nm::ParentRequest { parent_id: parent },
                |mut c, r| async move { c.default_colaborator_group(r).await },
            )
            .await
        }
        AdminCmd::ConsumerGroup { parent, grpc } => {
            nm_call(
                &grpc,
                nm::ParentRequest { parent_id: parent },
                |mut c, r| async move { c.default_consumer_group(r).await },
            )
            .await
        }
        AdminCmd::AdministratorGroup { parent, grpc } => {
            nm_call(
                &grpc,
                nm::ParentRequest { parent_id: parent },
                |mut c, r| async move { c.default_administrator_group(r).await },
            )
            .await
        }
        AdminCmd::Indexs { grpc } => {
            nm_call(&grpc, nm::EmptyRequest {}, |mut c, r| async move {
                c.indexs(r).await
            })
            .await
        }
        AdminCmd::DoclibTypes { grpc } => {
            nm_call(&grpc, nm::EmptyRequest {}, |mut c, r| async move {
                c.doc_libs_types(r).await
            })
            .await
        }
    }
}
