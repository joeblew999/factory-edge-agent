//! Distributed end-to-end test: a **real** gateway and a **real** edge agent, two
//! processes' worth of logic talking over OPC-UA. A job submitted to the gateway
//! is published to the agent, which runs the driver and writes the cut-list to
//! the machine — then reports back, and the gateway marks it complete.
//!
//! This proves the actual factory-floor topology works, not the in-process shortcut.

#![cfg(test)]

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use factory_gateway::{config::FactoryConfig, gateway::Gateway, opcua, registry::DriverRegistry};
use factory_machine_model::JobOrder;

use crate::config::AgentConfig;

const PORT: u16 = 4857;
const NS_URI: &str = "http://joeblew999.github.io/factory-floor/";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn job_flows_gateway_to_edge_agent_to_machine() {
    let usb_mount = std::env::temp_dir().join(format!("factory-edge-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&usb_mount);

    // ── Gateway: machine howick-1 in EDGE mode (run by a remote agent) ──────────
    let gw_toml = format!(
        r#"
        [factory]
        id = "test"
        name = "Edge Test"
        [opcua]
        host = "127.0.0.1"
        port = {PORT}
        namespace_uri = "{NS_URI}"
        [[machine]]
        id = "howick-1"
        driver = "howick-frama"
        edge = true
        [machine.identification]
        manufacturer = "Howick"
        model = "FRAMA"
    "#
    );
    let gw_config = FactoryConfig::from_toml(&gw_toml).unwrap();
    let registry = DriverRegistry::with_builtin_drivers();
    let gateway = Arc::new(Mutex::new(Gateway::build(&gw_config, &registry).unwrap()));
    tokio::spawn(opcua::serve(gw_config.opcua, gateway.clone()));
    tokio::time::sleep(Duration::from_millis(1500)).await; // gateway binds

    // ── Edge agent: connects to the gateway, drives howick-1, writes to usb_mount ─
    let agent_toml = format!(
        r#"
        gateway_url = "opc.tcp://127.0.0.1:{PORT}/"
        machine_id = "howick-1"
        namespace_uri = "{NS_URI}"
        [howick]
        usb_mount = "{mount}"
    "#,
        mount = usb_mount.display()
    );
    let agent_config: AgentConfig = toml::from_str(&agent_toml).unwrap();
    tokio::spawn(crate::run(agent_config));
    tokio::time::sleep(Duration::from_millis(2000)).await; // agent connects + subscribes

    // ── Dispatch a job to the gateway (as a MES would) ──────────────────────────
    let csv = "UNIT,MILLIMETRE\nW1,4740\n";
    gateway
        .lock()
        .await
        .submit("howick-1", JobOrder::with_payload("W1-1", "CutListCsv", csv.as_bytes().to_vec()))
        .unwrap();

    // ── Assert it reached the machine via the agent, and the gateway closed it ──
    let written = usb_mount.join("W1-1.csv");
    let mut got = None;
    for _ in 0..60 {
        if let Ok(c) = std::fs::read_to_string(&written) {
            got = Some(c);
            break;
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    let contents = got.expect("edge agent should have written the cut-list to the machine");
    assert_eq!(contents, csv, "the CSV reached the machine verbatim, over OPC-UA, via the edge agent");

    // The gateway should have seen the agent's ReportComplete and cleared the queue.
    let mut cleared = false;
    for _ in 0..20 {
        let g = gateway.lock().await;
        let m = &g.machines["howick-1"];
        if m.jobs.queue_depth() == 0 && m.published_job.is_none() {
            cleared = true;
            break;
        }
        drop(g);
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    let _ = std::fs::remove_dir_all(&usb_mount);
    assert!(cleared, "gateway should mark the job complete after the agent reports back");
}
