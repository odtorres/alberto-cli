pub mod admin;
pub mod config_cmd;
pub mod download;
pub mod node;
pub mod tenant;
pub mod upload;

use anyhow::{Context, Result};

/// Valida que un flag sea JSON antes de mandarlo al servidor.
pub(crate) fn valid_json(s: &str, flag: &str) -> Result<()> {
    serde_json::from_str::<serde_json::Value>(s)
        .map(|_| ())
        .with_context(|| format!("{flag} no es JSON valido"))
}
