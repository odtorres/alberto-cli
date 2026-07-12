//! `alberto config *` — init/list/show del archivo de perfiles.

use anyhow::{bail, Context, Result};

use crate::cli::ConfigCmd;
use crate::config::{config_path, load, DEFAULT_ENDPOINT};

const TEMPLATE: &str = r#"default_profile = "local"

[profiles.local]
endpoint = "http://127.0.0.1:9090"
api_key = ""
# download_dir = "~/Descargas"
"#;

pub fn run(cmd: ConfigCmd) -> Result<()> {
    match cmd {
        ConfigCmd::Init => {
            let path = config_path();
            if path.exists() {
                bail!("ya existe: {}", path.display());
            }
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)
                    .with_context(|| format!("creando {}", dir.display()))?;
            }
            std::fs::write(&path, TEMPLATE)
                .with_context(|| format!("escribiendo {}", path.display()))?;
            eprintln!("creado: {}", path.display());
            Ok(())
        }
        ConfigCmd::List => {
            let cfg = load()?;
            for name in cfg.profiles.keys() {
                let marker = if cfg.default_profile.as_deref() == Some(name) {
                    " (default)"
                } else {
                    ""
                };
                println!("{name}{marker}");
            }
            Ok(())
        }
        ConfigCmd::Show { profile } => {
            let cfg = load()?;
            let name = profile
                .or(cfg.default_profile.clone())
                .context("no hay perfil: pasa uno o define default_profile")?;
            let p = cfg
                .profiles
                .get(&name)
                .with_context(|| format!("perfil '{name}' no existe"))?;
            println!("perfil:   {name}");
            println!(
                "endpoint: {}",
                p.endpoint.as_deref().unwrap_or(DEFAULT_ENDPOINT)
            );
            println!("api_key:  {}", mask(p.api_key.as_deref().unwrap_or("")));
            if let Some(dir) = &p.download_dir {
                println!("download_dir: {dir}");
            }
            Ok(())
        }
    }
}

fn mask(key: &str) -> String {
    if key.is_empty() {
        "(sin definir)".into()
    } else if key.chars().count() <= 4 {
        "…".into()
    } else {
        format!("{}…", key.chars().take(4).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_no_paniquea_con_utf8() {
        assert_eq!(mask(""), "(sin definir)");
        assert_eq!(mask("abcd"), "…");
        assert_eq!(mask("ññññññ"), "ññññ…");
        assert_eq!(mask("supersecreta"), "supe…");
    }
}
