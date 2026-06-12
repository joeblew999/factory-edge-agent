//! Driver registry — maps a config `driver = "..."` to a constructor that builds
//! the driver. This is what keeps it at **one binary**: the agent runs whichever
//! driver the config names, so a new machine type is a new `factory-<machine>-driver`
//! crate registered here — never a new agent binary.

use std::collections::HashMap;

use factory_machine_model::{BoxedDriver, Identification};
use factory_howick_driver::{HowickConfig, HowickFrama};

use crate::config::AgentConfig;

/// Builds a boxed driver from the agent's config.
pub type Constructor = fn(&AgentConfig) -> anyhow::Result<BoxedDriver>;

/// Registry of drivers this agent binary was built to support.
pub struct DriverRegistry {
    constructors: HashMap<&'static str, Constructor>,
}

impl DriverRegistry {
    /// A registry with every driver compiled into this agent.
    pub fn with_builtin_drivers() -> Self {
        let mut r = Self {
            constructors: HashMap::new(),
        };
        r.register(factory_howick_driver::KIND, build_howick);
        r
    }

    pub fn register(&mut self, kind: &'static str, ctor: Constructor) {
        self.constructors.insert(kind, ctor);
    }

    /// Instantiate the driver named in the config, or error if unknown.
    pub fn build(&self, config: &AgentConfig) -> anyhow::Result<BoxedDriver> {
        let ctor = self
            .constructors
            .get(config.driver.as_str())
            .ok_or_else(|| anyhow::anyhow!("no driver registered for kind '{}'", config.driver))?;
        ctor(config)
    }

    pub fn supported_kinds(&self) -> Vec<&'static str> {
        let mut v: Vec<_> = self.constructors.keys().copied().collect();
        v.sort_unstable();
        v
    }
}

/// Constructor for the Howick FRAMA driver — reads the `[howick]` config table.
fn build_howick(config: &AgentConfig) -> anyhow::Result<BoxedDriver> {
    let cfg: HowickConfig = config.driver_config("howick")?;
    Ok(Box::new(HowickFrama::new(
        config.machine_id.clone(),
        Identification::new("Howick", "FRAMA"),
        cfg,
    )))
}
