//! Edge agent config — one file per box wired to a machine.
//!
//! ```toml
//! gateway_url   = "opc.tcp://gateway.local:4840/"
//! machine_id    = "howick-1"
//! namespace_uri = "http://joeblew999.github.io/factory-floor/"
//!
//! [howick]                       # the driver's config (factory-howick-driver)
//! usb_mount       = "/mnt/usb_share"
//! usb_gadget_mode = true
//! coil_sensor     = true
//! ```

use factory_howick_driver::HowickConfig;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AgentConfig {
    /// OPC-UA endpoint of the factory-gateway.
    pub gateway_url: String,
    /// Which machine in the gateway this agent drives (the `Machines/<id>` key).
    pub machine_id: String,
    /// The gateway's namespace URI (to resolve the namespace index).
    #[serde(default = "default_ns")]
    pub namespace_uri: String,
    /// Howick driver config (this agent currently hosts the Howick driver).
    #[serde(default)]
    pub howick: HowickConfig,
}

fn default_ns() -> String {
    "http://joeblew999.github.io/factory-floor/".to_owned()
}

impl AgentConfig {
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        Ok(toml::from_str(&std::fs::read_to_string(path)?)?)
    }
}
