//! `alberto node *` — operaciones de nodos vía NodeManagerService.

use anyhow::Result;

use super::valid_json;
use crate::cli::NodeCmd;
use crate::client::{nm_call, nodemanager as nm};

pub async fn run(cmd: NodeCmd) -> Result<()> {
    match cmd {
        NodeCmd::Get { id, grpc } => {
            nm_call(
                &grpc,
                nm::UniqueIdRequest { unique_id: id },
                |mut c, r| async move { c.node_get(r).await },
            )
            .await
        }
        NodeCmd::Ids {
            ids,
            node_type,
            grpc,
        } => {
            nm_call(
                &grpc,
                nm::IdsRequest {
                    ids,
                    r#type: node_type,
                },
                |mut c, r| async move { c.ids(r).await },
            )
            .await
        }
        NodeCmd::ByType {
            node_type,
            tenant,
            grpc,
        } => {
            nm_call(
                &grpc,
                nm::HomeRequest {
                    tenant,
                    r#type: node_type,
                },
                |mut c, r| async move { c.by_type(r).await },
            )
            .await
        }
        NodeCmd::ByPath { path, tenant, grpc } => {
            nm_call(
                &grpc,
                nm::ByPathRequest { tenant, path },
                |mut c, r| async move { c.by_path(r).await },
            )
            .await
        }
        NodeCmd::Children {
            id,
            secondary,
            grpc,
        } => {
            nm_call(
                &grpc,
                nm::NodeChildRequest {
                    unique_id: id,
                    secondary,
                },
                |mut c, r| async move { c.node_child(r).await },
            )
            .await
        }
        NodeCmd::User { username, grpc } => {
            nm_call(
                &grpc,
                nm::UserRequest {
                    username,
                    password: String::new(),
                },
                |mut c, r| async move { c.user(r).await },
            )
            .await
        }
        NodeCmd::Datamerge { id, data, grpc } => {
            valid_json(&data, "--data")?;
            nm_call(
                &grpc,
                nm::DatamergeRequest {
                    unique_id: id,
                    data_json: data,
                },
                |mut c, r| async move { c.datamerge(r).await },
            )
            .await
        }
        NodeCmd::DataUpdate { id, data, grpc } => {
            valid_json(&data, "--data")?;
            nm_call(
                &grpc,
                nm::DataUpdateRequest {
                    unique_id: id,
                    data_json: data,
                },
                |mut c, r| async move { c.data_update(r).await },
            )
            .await
        }
        NodeCmd::BulkDatamerge { changes, grpc } => {
            valid_json(&changes, "--changes")?;
            nm_call(
                &grpc,
                nm::BulkDatamergeRequest {
                    changes_json: changes,
                },
                |mut c, r| async move { c.bulk_datamerge(r).await },
            )
            .await
        }
        NodeCmd::Patch {
            envelope_path,
            path,
            data,
            grpc,
        } => {
            valid_json(&data, "--data")?;
            nm_call(
                &grpc,
                nm::PatchRequest {
                    envelope_path,
                    path,
                    data_json: data,
                },
                |mut c, r| async move { c.patch(r).await },
            )
            .await
        }
        NodeCmd::GetIn {
            node_path,
            path,
            grpc,
        } => {
            nm_call(
                &grpc,
                nm::GetRequest { node_path, path },
                |mut c, r| async move { c.get(r).await },
            )
            .await
        }
        NodeCmd::ByName { name, grpc } => {
            nm_call(&grpc, nm::NameRequest { name }, |mut c, r| async move {
                c.node_by_name(r).await
            })
            .await
        }
        NodeCmd::Create {
            parent,
            node_type,
            data,
            grpc,
        } => {
            valid_json(&data, "--data")?;
            nm_call(
                &grpc,
                nm::NodeCreateRequest {
                    parent_id: parent,
                    data_json: data,
                    r#type: node_type,
                },
                |mut c, r| async move { c.node_create(r).await },
            )
            .await
        }
        NodeCmd::AddSecondary {
            child_id,
            parent_id,
            grpc,
        } => {
            nm_call(
                &grpc,
                nm::SecondaryParentRequest {
                    child_id,
                    parent_id,
                },
                |mut c, r| async move { c.add_secondary_parent(r).await },
            )
            .await
        }
        NodeCmd::RemoveSecondary {
            child_id,
            parent_id,
            grpc,
        } => {
            nm_call(
                &grpc,
                nm::SecondaryParentRequest {
                    child_id,
                    parent_id,
                },
                |mut c, r| async move { c.remove_secondary_parent(r).await },
            )
            .await
        }
    }
}
