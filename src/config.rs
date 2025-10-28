// config.rs

use argmap::List;
use config::{Config, ConfigError, File};
use serde_derive::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize, Clone, Default)]
#[allow(unused)]
pub struct BitcoinCoreSettings {
    pub host: String,
    pub rpc_port: String,
    pub rpc_user: String,
    pub rpc_password: String,
    pub zmq_port: String,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[allow(unused)]
pub struct CoreLightningSettings {
    pub rest_address: String,
    pub rest_rune: String,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[allow(unused)]
pub struct LndSettings {
    pub rest_address: String,
    pub macaroon_hex: String,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(unused)]
pub struct PriceSettings {
    pub enabled: bool,
    pub currency: String,
    pub big_text: bool,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(unused)]
pub struct FeesSettings {
    pub enabled: bool,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(unused)]
pub struct NodeConfig {
    pub provider: String,
    pub bitcoin_core: Option<BitcoinCoreSettings>,
    pub core_lightning: Option<CoreLightningSettings>,
    pub lnd: Option<LndSettings>,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(unused)]
pub struct AppConfig {
    pub tick_rate: String,
    pub price: PriceSettings,
    pub fees: FeesSettings,
    pub bitcoin_core: BitcoinCoreSettings,
    pub core_lightning: CoreLightningSettings,
    pub lnd: LndSettings,
    #[serde(default)]
    pub nodes: Vec<NodeConfig>,
}

fn match_string_to_bool(value: &str) -> bool {
    match value {
        "true" | "1" => true,
        "false" | "0" => false,
        _ => false,
    }
}

impl AppConfig {
    pub fn new(_args: List, argv: HashMap<String, Vec<String>>) -> Result<Self, ConfigError> {
        let homedir = home::home_dir().unwrap();
        let home_path = homedir.as_path().to_str();

        let mut s = Config::builder()
            // general
            .set_default("tick_rate", 250)?
            // bitcoin core defaults
            .set_default("bitcoin_core.host", "localhost")?
            .set_default("bitcoin_core.rpc_port", 8332)?
            .set_default("bitcoin_core.rpc_user", "username")?
            .set_default("bitcoin_core.rpc_password", "password")?
            .set_default("bitcoin_core.zmq_port", 28332)?
            // core lightning defaults
            .set_default("core_lightning.rest_address", "https://127.0.0.1:9835")?
            .set_default("core_lightning.rest_rune", "")?
            // lnd defaults
            .set_default("lnd.rest_address", "https://localhost:8080")?
            .set_default("lnd.macaroon_hex", "")?
            // price
            .set_default("price.enabled", true)?
            .set_default("price.big_text", true)?
            .set_default("price.currency", "USD")?
            // fees
            .set_default("fees.enabled", true)?;

        let mut default_config_file: String = String::from("/etc/btcmon/btcmon.toml");

        let config_file = match (argv.contains_key("c"), argv.contains_key("config")) {
            (true, false) => argv
                .get("c")
                .and_then(|v| Some(v.first().unwrap().as_str()))
                .unwrap(),
            (false, true) | (true, true) => argv
                .get("config")
                .and_then(|v| Some(v.first().unwrap().as_str()))
                .unwrap(),
            _ => match home_path {
                Some(home_path) => {
                    default_config_file = vec![home_path, "/.btcmon/btcmon.toml"].join("");
                    default_config_file.as_str()
                }
                _ => default_config_file.as_str(),
            },
        };

        s = s.add_source(File::with_name(config_file).required(false));

        let args = argv.clone();
        for key in argv.into_keys() {
            if let Some(value) = args
                .get(&key)
                .and_then(|v| Some(v.first().unwrap().as_str()))
            {
                match key.as_str() {
                    "price.enabled" | "fees.enabled" => {
                        s = s.set_override(key, match_string_to_bool(value))?;
                    }
                    _ => {
                        s = s.set_override(key, value.to_string())?;
                    }
                }
            }
        }

        s.build()?.try_deserialize()
    }
}