use std::str::FromStr;

use zcash_protocol::consensus::Network;

/// The networks Vintage supports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VintageNetwork {
    Main,
    Test,
}

impl VintageNetwork {
    pub fn params(self) -> Network {
        match self {
            VintageNetwork::Main => Network::MainNetwork,
            VintageNetwork::Test => Network::TestNetwork,
        }
    }

    /// Default public lightwalletd/Zaino endpoint (the zcash-devtool `zecrocks` presets).
    pub fn default_server(self) -> &'static str {
        match self {
            VintageNetwork::Main => "zec.rocks:443",
            VintageNetwork::Test => "testnet.zec.rocks:443",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            VintageNetwork::Main => "main",
            VintageNetwork::Test => "test",
        }
    }
}

impl FromStr for VintageNetwork {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "main" | "mainnet" => Ok(VintageNetwork::Main),
            "test" | "testnet" => Ok(VintageNetwork::Test),
            other => Err(format!("unknown network `{other}` (expected main or test)")),
        }
    }
}
