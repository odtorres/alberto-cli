//! Perfiles de conexión: ~/.config/alberto/config.toml
//!
//! Precedencia: flag/env (clap los une) > --profile/ALBERTO_PROFILE >
//! default_profile del archivo > defaults de compilación.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:9090";

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    pub default_profile: Option<String>,
    #[serde(default)]
    pub profiles: BTreeMap<String, Profile>,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct Profile {
    pub endpoint: Option<String>,
    pub api_key: Option<String>,
    pub download_dir: Option<String>,
}

pub fn config_path() -> PathBuf {
    if let Some(p) = std::env::var_os("ALBERTO_CONFIG") {
        return p.into();
    }
    dirs::home_dir()
        .unwrap_or_default()
        .join(".config/alberto/config.toml")
}

pub fn load() -> Result<Config> {
    load_from(&config_path())
}

pub fn load_from(path: &Path) -> Result<Config> {
    if !path.exists() {
        return Ok(Config::default());
    }
    let text =
        std::fs::read_to_string(path).with_context(|| format!("leyendo {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("TOML invalido en {}", path.display()))
}

/// Resuelve (endpoint, api_key) combinando flags/env con el perfil.
pub fn resolve(
    cfg: &Config,
    profile: Option<&str>,
    endpoint: Option<String>,
    api_key: Option<String>,
) -> Result<(String, String)> {
    let prof = match profile.or(cfg.default_profile.as_deref()) {
        Some(name) => Some(
            cfg.profiles
                .get(name)
                .with_context(|| {
                    format!("perfil '{name}' no existe en {}", config_path().display())
                })?
                .clone(),
        ),
        None => None,
    };

    let endpoint = endpoint
        .or_else(|| prof.as_ref().and_then(|p| p.endpoint.clone()))
        .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string());

    let Some(api_key) = api_key.or_else(|| prof.as_ref().and_then(|p| p.api_key.clone())) else {
        bail!(
            "falta el api key: usa --api-key, ALBERTO_API_KEY, o api_key en el perfil ({})",
            config_path().display()
        );
    };

    Ok((endpoint, api_key))
}

/// Expande "~/" al home del usuario; el resto queda igual.
pub fn expand_home(path: &str) -> PathBuf {
    if path == "~" {
        return dirs::home_dir().unwrap_or_default();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return dirs::home_dir().unwrap_or_default().join(rest);
    }
    PathBuf::from(path)
}

/// download_dir del perfil elegido (o del default_profile). No valida
/// nombres: un perfil inexistente simplemente no aporta valor.
pub fn profile_download_dir(cfg: &Config, profile: Option<&str>) -> Option<String> {
    let name = profile.or(cfg.default_profile.as_deref())?;
    cfg.profiles.get(name)?.download_dir.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_with(default: Option<&str>, profiles: &[(&str, &str, &str)]) -> Config {
        Config {
            default_profile: default.map(String::from),
            profiles: profiles
                .iter()
                .map(|(n, e, k)| {
                    (
                        n.to_string(),
                        Profile {
                            endpoint: Some(e.to_string()),
                            api_key: Some(k.to_string()),
                            ..Default::default()
                        },
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn flag_gana_a_perfil() {
        let cfg = cfg_with(None, &[("qa", "http://qa:9090", "kqa")]);
        let (e, k) = resolve(
            &cfg,
            Some("qa"),
            Some("http://flag:1".into()),
            Some("kflag".into()),
        )
        .unwrap();
        assert_eq!(e, "http://flag:1");
        assert_eq!(k, "kflag");
    }

    #[test]
    fn perfil_llena_lo_que_falta() {
        let cfg = cfg_with(None, &[("qa", "http://qa:9090", "kqa")]);
        let (e, k) = resolve(&cfg, Some("qa"), None, None).unwrap();
        assert_eq!(e, "http://qa:9090");
        assert_eq!(k, "kqa");
    }

    #[test]
    fn default_profile_aplica_sin_flag() {
        let cfg = cfg_with(Some("qa"), &[("qa", "http://qa:9090", "kqa")]);
        let (e, k) = resolve(&cfg, None, None, None).unwrap();
        assert_eq!(e, "http://qa:9090");
        assert_eq!(k, "kqa");
    }

    #[test]
    fn perfil_inexistente_es_error() {
        let cfg = cfg_with(None, &[]);
        assert!(resolve(&cfg, Some("nope"), None, None).is_err());
    }

    #[test]
    fn sin_api_key_es_error_con_pista() {
        let cfg = Config::default();
        let err = resolve(&cfg, None, Some("http://x:1".into()), None).unwrap_err();
        assert!(err.to_string().contains("api key"));
    }

    #[test]
    fn endpoint_default_cuando_no_hay_nada() {
        let cfg = Config::default();
        let (e, _) = resolve(&cfg, None, None, Some("k".into())).unwrap();
        assert_eq!(e, DEFAULT_ENDPOINT);
    }

    #[test]
    fn load_from_lee_toml() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("c.toml");
        std::fs::write(
            &p,
            "default_profile = \"qa\"\n[profiles.qa]\nendpoint = \"http://qa:1\"\napi_key = \"s\"\n",
        )
        .unwrap();
        let cfg = load_from(&p).unwrap();
        assert_eq!(cfg.default_profile.as_deref(), Some("qa"));
        assert_eq!(cfg.profiles["qa"].endpoint.as_deref(), Some("http://qa:1"));
    }

    #[test]
    fn load_from_inexistente_es_default() {
        let cfg = load_from(Path::new("/no/existe/c.toml")).unwrap();
        assert!(cfg.profiles.is_empty());
    }

    #[test]
    fn expand_home_expande_tilde() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(expand_home("~/Descargas"), home.join("Descargas"));
        assert_eq!(expand_home("/abs/x"), PathBuf::from("/abs/x"));
        assert_eq!(expand_home("rel/x"), PathBuf::from("rel/x"));
        assert_eq!(expand_home("~"), home);
    }

    #[test]
    fn profile_download_dir_usa_perfil_o_default() {
        let mut cfg = cfg_with(Some("qa"), &[("qa", "http://qa:1", "k")]);
        assert_eq!(profile_download_dir(&cfg, None), None);
        cfg.profiles.get_mut("qa").unwrap().download_dir = Some("~/dl".into());
        assert_eq!(profile_download_dir(&cfg, None), Some("~/dl".into()));
        assert_eq!(profile_download_dir(&cfg, Some("qa")), Some("~/dl".into()));
        assert_eq!(profile_download_dir(&cfg, Some("nope")), None);
    }
}
