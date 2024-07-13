use argmap::List;
use config::{Config, ConfigError, File};
use serde_derive::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize, Clone)]
#[allow(unused)]
pub struct BitcoinCoreSettings {
    pub host: String,
    pub rpc_port: String,
    pub rpc_user: String,
    pub rpc_password: String,
    pub zmq_hashblock_port: String,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(unused)]
pub struct PriceSettings {
    pub enabled: bool,
    pub currency: String,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(unused)]
pub struct AppConfig {
    pub tick_rate: String,
    pub price: PriceSettings,
    pub bitcoin_core: BitcoinCoreSettings,
}

fn match_string_to_bool(value: &str) -> bool {
    match value {
        "true" => true,
        "1" => true,
        "false" => false,
        "0" => false,
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
            // bitcoin core
            .set_default("bitcoin_core.host", "localhost")?
            .set_default("bitcoin_core.rpc_port", 8332)?
            .set_default("bitcoin_core.rpc_user", "username")?
            .set_default("bitcoin_core.rpc_password", "password")?
            .set_default("bitcoin_core.zmq_hashblock_port", 28332)?
            // price
            .set_default("price.enabled", true)?
            .set_default("price.currency", "USD")?;

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
                    "price.enabled" => {
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
