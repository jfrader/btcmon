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
    pub variation: String,
    pub variation_threshold: f64,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(unused)]
pub struct FeesSettings {
    pub enabled: bool,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(unused)]
pub struct TouchSettings {
    pub enabled: bool,
    pub device: String,
    pub swap_xy: bool,
    pub invert_x: bool,
    pub invert_y: bool,
}

impl Default for TouchSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            device: String::new(),
            swap_xy: true,
            invert_x: true,
            invert_y: false,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[allow(unused)]
pub struct NodeConfig {
    #[serde(default)]
    pub name: Option<String>,
    pub provider: String,
    pub bitcoin_core: Option<BitcoinCoreSettings>,
    pub core_lightning: Option<CoreLightningSettings>,
    pub lnd: Option<LndSettings>,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(unused)]
pub struct AppConfig {
    pub tick_rate: String,
    pub streamer_mode: bool,
    pub node_switch_interval: String, // New field for rotation time in seconds
    pub price: PriceSettings,
    pub fees: FeesSettings,
    pub bitcoin_core: BitcoinCoreSettings,
    pub core_lightning: CoreLightningSettings,
    pub lnd: LndSettings,
    #[serde(default)]
    pub nodes: Vec<NodeConfig>,
    #[serde(default)]
    pub touch: TouchSettings,
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
            .set_default("streamer_mode", false)?
            .set_default("node_switch_interval", "5")? // Default rotation time of 5 seconds
            // bitcoin core defaults (will be cleared if nodes is used)
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
            .set_default("price.variation", "minute")?
            .set_default("price.variation_threshold", 0.0)?
            // fees
            .set_default("fees.enabled", true)?
            .set_default("touch.enabled", true)?
            .set_default("touch.device", "")?
            .set_default("touch.swap_xy", true)?
            .set_default("touch.invert_x", true)?
            .set_default("touch.invert_y", false)?;

        let mut default_config_file: String = String::from("/etc/btcmon/btcmon.toml");

        let config_file = match (argv.contains_key("c"), argv.contains_key("config")) {
            (true, false) => argv
                .get("c")
                .and_then(|v| Some(v.first().unwrap().as_str()))
                .unwrap()
                .to_string(),
            (false, true) | (true, true) => argv
                .get("config")
                .and_then(|v| Some(v.first().unwrap().as_str()))
                .unwrap()
                .to_string(),
            _ => match home_path {
                Some(home_path) => {
                    default_config_file = vec![home_path, "/.btcmon/btcmon.toml"].join("");
                    default_config_file
                }
                _ => default_config_file,
            },
        };

        s = s.add_source(File::with_name(&config_file).required(false));

        let args = argv.clone();
        for key in argv.into_keys() {
            if let Some(value) = args
                .get(&key)
                .and_then(|v| Some(v.first().unwrap().as_str()))
            {
                match key.as_str() {
                    "price.enabled" | "fees.enabled" | "streamer_mode" | "touch.enabled"
                    | "touch.swap_xy" | "touch.invert_x" | "touch.invert_y" => {
                        s = s.set_override(key, match_string_to_bool(value))?;
                    }
                    _ => {
                        s = s.set_override(key, value.to_string())?;
                    }
                }
            }
        }

        let mut config: AppConfig = s.build()?.try_deserialize()?;

        let has_node_args = args.keys().any(|key| {
            key.starts_with("bitcoin_core.")
                || key.starts_with("core_lightning.")
                || key.starts_with("lnd.")
                || key.starts_with("nodes")
                || key.starts_with("node.")
        });
        let file_has_node_settings = std::fs::read_to_string(&config_file)
            .map(|contents| {
                contents.contains("[bitcoin_core]")
                    || contents.contains("[core_lightning]")
                    || contents.contains("[lnd]")
                    || contents.contains("[nodes]")
                    || contents.contains("[node]")
            })
            .unwrap_or(false);

        if config.nodes.is_empty() && !has_node_args && !file_has_node_settings {
            config.bitcoin_core = BitcoinCoreSettings::default();
            config.core_lightning = CoreLightningSettings::default();
            config.lnd = LndSettings::default();
        }

        // Clear legacy providers if nodes array is used
        if !config.nodes.is_empty() {
            config.bitcoin_core = BitcoinCoreSettings::default();
            config.core_lightning = CoreLightningSettings::default();
            config.lnd = LndSettings::default();
        }

        Ok(config)
    }
}
