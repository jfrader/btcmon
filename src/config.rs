use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct Config {
    pub bitcoin_core_host: String,
    pub bitcoin_core_rpc_port: u16,
    pub bitcoin_core_rpc_user: String,
    pub bitcoin_core_rpc_password: String,
    pub bitcoin_core_zmq_hashblock_port: u16,
}

pub trait ConfigProvider {
    fn get_config(&self) -> &Config;
}

#[derive(Clone)]
pub struct CmdConfigProvider(Config);

impl CmdConfigProvider {
    pub fn new(_args: Vec<String>, argv: HashMap<String, Vec<String>>) -> Self {
        let bitcoin_core_host = argv
            .get("bitcoin_core_host")
            .unwrap_or(&vec!["localhost".to_string()])
            .to_vec()
            .to_vec();
        let bitcoin_core_rpc_port = argv
            .get("bitcoin_core_rpc_port")
            .unwrap_or(&vec!["8332".to_string()])
            .to_vec()
            .to_vec();
        let bitcoin_core_zmq_hashblock_port = argv
            .get("bitcoin_core_zmq_hashblock_port")
            .unwrap_or(&vec!["28332".to_string()])
            .to_vec()
            .to_vec();
        let bitcoin_core_rpc_user = argv
            .get("bitcoin_core_rpc_user")
            .unwrap_or(&vec!["username".to_string()])
            .to_vec()
            .to_vec();
        let bitcoin_core_rpc_password = argv
            .get("bitcoin_core_rpc_password")
            .unwrap_or(&vec!["password".to_string()])
            .to_vec()
            .to_vec();

        let config = Config {
            bitcoin_core_host: bitcoin_core_host.first().unwrap().to_string(),
            bitcoin_core_rpc_port: bitcoin_core_rpc_port
                .first()
                .unwrap()
                .parse::<u16>()
                .unwrap(),
            bitcoin_core_rpc_user: bitcoin_core_rpc_user.first().unwrap().to_string(),
            bitcoin_core_rpc_password: bitcoin_core_rpc_password.first().unwrap().to_string(),
            bitcoin_core_zmq_hashblock_port: bitcoin_core_zmq_hashblock_port
                .first()
                .unwrap()
                .parse::<u16>()
                .unwrap(),
        };
        CmdConfigProvider(config)
    }
}

impl ConfigProvider for CmdConfigProvider {
    fn get_config(&self) -> &Config {
        &self.0
    }
}

impl Default for CmdConfigProvider {
    fn default() -> Self {
        Self::new(Vec::new(), HashMap::new())
    }
}
