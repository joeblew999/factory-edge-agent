//! Edge agent config — one file per box wired to a machine.
//!
//! ```toml
//! gateway_url   = "opc.tcp://gateway.local:4840/"
//! machine_id    = "howick-1"
//! driver        = "howick-frama"        # which driver this agent runs
//! namespace_uri = "http://joeblew999.github.io/factory-floor/"
//!
//! [howick]                       # the driver's own config (typed by the driver)
//! usb_mount       = "/mnt/usb_share"
//! usb_gadget_mode = true
//! coil_sensor     = true
//! ```
//!
//! The agent picks the driver named in `driver` from its registry — so one
//! edge-agent binary runs *any* machine type; adding a machine type is a new
//! driver crate, never a new binary.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AgentConfig {
    /// OPC-UA endpoint of the factory-gateway.
    pub gateway_url: String,
    /// Which machine in the gateway this agent drives (the `Machines/<id>` key).
    pub machine_id: String,
    /// Which driver kind to run (matches a registered driver, e.g. `"howick-frama"`).
    pub driver: String,
    /// The gateway's namespace URI (to resolve the namespace index).
    #[serde(default = "default_ns")]
    pub namespace_uri: String,
    /// Driver-specific `[<key>]` tables (e.g. `[howick]`) — opaque here, parsed
    /// by the driver's constructor.
    #[serde(flatten)]
    pub extra: toml::Table,
}

fn default_ns() -> String {
    "http://joeblew999.github.io/factory-floor/".to_owned()
}

impl AgentConfig {
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        Ok(toml::from_str(&std::fs::read_to_string(path)?)?)
    }

    /// The driver-specific config table under `key` (e.g. `"howick"`),
    /// deserialized into the driver's typed config.
    pub fn driver_config<T: serde::de::DeserializeOwned>(&self, key: &str) -> anyhow::Result<T> {
        match self.extra.get(key) {
            Some(v) => Ok(v.clone().try_into()?),
            None => Ok(toml::Value::Table(toml::Table::new()).try_into()?),
        }
    }
}
