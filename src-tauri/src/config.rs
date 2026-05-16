use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::store::sqlite::SqliteStore;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AppConfig {
    pub device_id: String,
    pub device_name: String,
    pub listen_host: String,
    pub listen_port: u16,
    pub download_dir: String,
    pub protocol_version: u16,
    pub version: String,
}

pub struct LoadedConfig {
    pub config: AppConfig,
    pub config_path: PathBuf,
    pub store: SqliteStore,
}

pub fn load_or_init() -> Result<LoadedConfig, String> {
    let config_dir = config_dir()?;
    let config_path = config_dir.join("config.toml");
    fs::create_dir_all(&config_dir)
        .map_err(|err| format!("create config directory failed: {err}"))?;

    if config_path.exists() {
        let raw = fs::read_to_string(&config_path)
            .map_err(|err| format!("read config file failed: {err}"))?;
        let mut config: AppConfig =
            toml::from_str(&raw).map_err(|err| format!("parse config file failed: {err}"))?;
        normalize_config(&mut config);
        save_config(&config_path, &config)?;
        let store = SqliteStore::open(config_dir.join("zerodrop.db"))?;
        return Ok(LoadedConfig {
            config,
            config_path,
            store,
        });
    }

    let config = default_config(&config_dir);
    save_config(&config_path, &config)?;
    let store = SqliteStore::open(config_dir.join("zerodrop.db"))?;

    Ok(LoadedConfig {
        config,
        config_path,
        store,
    })
}

fn config_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "cannot resolve home directory".to_string())?;
    Ok(home.join(".zerodrop"))
}

fn default_config(config_dir: &std::path::Path) -> AppConfig {
    let device_id = format!("zd_{}", Uuid::new_v4().simple());
    let suffix = device_id
        .strip_prefix("zd_")
        .unwrap_or(&device_id)
        .chars()
        .take(6)
        .collect::<String>();
    let device_name = default_device_name().unwrap_or_else(|| format!("ZeroDrop-{suffix}"));
    let download_dir = default_download_dir(config_dir);

    AppConfig {
        device_id,
        device_name,
        listen_host: "0.0.0.0".to_string(),
        listen_port: 8765,
        download_dir,
        protocol_version: 1,
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

fn normalize_config(config: &mut AppConfig) {
    if config.device_id.trim().is_empty() {
        config.device_id = format!("zd_{}", Uuid::new_v4().simple());
    }

    if config.device_name.trim().is_empty() {
        config.device_name = default_device_name().unwrap_or_else(|| "ZeroDrop".to_string());
    }

    if config.listen_host.trim().is_empty() {
        config.listen_host = "0.0.0.0".to_string();
    }

    if config.listen_port == 0 {
        config.listen_port = 8765;
    }

    if config.download_dir.trim().is_empty() {
        config.download_dir =
            default_download_dir(&config_dir().unwrap_or_else(|_| PathBuf::from(".")));
    }

    if config.protocol_version == 0 {
        config.protocol_version = 1;
    }

    config.version = env!("CARGO_PKG_VERSION").to_string();
}

fn default_device_name() -> Option<String> {
    ["COMPUTERNAME", "HOSTNAME"]
        .iter()
        .find_map(|key| std::env::var(key).ok())
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
}

fn default_download_dir(config_dir: &std::path::Path) -> String {
    dirs::download_dir()
        .unwrap_or_else(|| config_dir.join("downloads"))
        .join("ZeroDrop")
        .display()
        .to_string()
}

fn save_config(config_path: &std::path::Path, config: &AppConfig) -> Result<(), String> {
    let raw =
        toml::to_string_pretty(config).map_err(|err| format!("serialize config failed: {err}"))?;
    fs::write(config_path, raw).map_err(|err| format!("write config file failed: {err}"))
}
