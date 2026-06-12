//! # factory-edge-agent
//!
//! Runs a machine driver **at the edge** — on the box physically wired to the
//! machine — and connects to [factory-gateway] over OPC-UA as a client. This is
//! the real factory-floor topology: machines are distributed across the floor,
//! so each runs its own agent process that talks back to one central gateway.
//!
//! Flow (mirrors the standard SCADA-to-PLC pattern: subscribe, don't poll):
//!   1. Connect to the gateway, resolve the namespace.
//!   2. **Subscribe** to `Machines/<id>/EdgeAgent/PendingJobId` — the gateway
//!      pushes the instant a job is dispatched to this machine.
//!   3. Read `PendingJobCsv`, run the local driver (chosen from the [`registry`]
//!      by the config's `driver`), then call `JobOrderReceiver/ReportComplete`.
//!
//! One binary runs **any** machine type — adding one is a new driver crate
//! registered in [`registry`], never a new agent binary.
//!
//! [factory-gateway]: https://github.com/joeblew999/factory-gateway

pub mod config;
pub mod registry;

#[cfg(test)]
mod e2e;

use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use opcua::client::{ClientBuilder, DataChangeCallback, IdentityToken, MonitoredItem};
use opcua::crypto::SecurityPolicy;
use opcua::types::{
    AttributeId, DataValue, MessageSecurityMode, MonitoredItemCreateRequest, NodeId, ReadValueId,
    TimestampsToReturn, UserTokenPolicy, VariableId, Variant,
};

use factory_machine_model::JobOrder;

use config::AgentConfig;
use registry::DriverRegistry;

/// Connect to the gateway and process jobs for this machine until the connection
/// ends. Returns `Ok(())` only if the run loop exits cleanly.
pub async fn run(config: AgentConfig) -> anyhow::Result<()> {
    let mid = config.machine_id.clone();

    // Pick the driver named in the config — one binary runs any machine type.
    let registry = DriverRegistry::with_builtin_drivers();
    let driver = registry.build(&config).map_err(|e| {
        anyhow::anyhow!("{e} (this agent supports: {:?})", registry.supported_kinds())
    })?;

    let url = config.gateway_url.trim_end_matches('/').to_string();
    tracing::info!(%url, machine = %mid, driver = %config.driver, "edge agent connecting to gateway");

    let mut client = ClientBuilder::new()
        .application_name("factory-edge-agent")
        .application_uri("urn:factory-edge-agent")
        .trust_server_certs(true)
        .create_sample_keypair(true)
        .session_retry_limit(-1)
        .client()
        .map_err(|e| anyhow::anyhow!("client build: {e:?}"))?;

    let (session, event_loop) = client
        .connect_to_matching_endpoint(
            (
                url.as_str(),
                SecurityPolicy::None.to_str(),
                MessageSecurityMode::None,
                UserTokenPolicy::anonymous(),
            ),
            IdentityToken::Anonymous,
        )
        .await
        .map_err(|e| anyhow::anyhow!("connect {url}: {e:?}"))?;
    let _loop_handle = event_loop.spawn();
    session.wait_for_connection().await;
    tracing::info!("connected to gateway ✓");

    let ns = resolve_ns(&session, &config.namespace_uri).await.unwrap_or(2);

    let pending_csv_node = NodeId::new(ns, format!("Machines/{mid}/EdgeAgent/PendingJobCsv"));
    let receiver_node = NodeId::new(ns, format!("Machines/{mid}/JobOrderReceiver"));
    let report_node = NodeId::new(ns, format!("Machines/{mid}/JobOrderReceiver/ReportComplete"));

    // Subscription callback (sync) → run loop (async): hand over the new job id.
    let pending: Arc<StdMutex<Option<String>>> = Arc::new(StdMutex::new(None));
    let notify = Arc::new(tokio::sync::Notify::new());
    let (p, n) = (pending.clone(), notify.clone());

    let sub = session
        .create_subscription(
            Duration::from_millis(500),
            10,
            30,
            0,
            0,
            true,
            DataChangeCallback::new(move |dv: DataValue, _item: &MonitoredItem| {
                if let Some(Variant::String(s)) = dv.value {
                    let id = s.value().clone().unwrap_or_default();
                    if !id.is_empty() {
                        *p.lock().unwrap() = Some(id);
                        n.notify_one();
                    }
                }
            }),
        )
        .await
        .map_err(|e| anyhow::anyhow!("create_subscription: {e:?}"))?;
    session
        .create_monitored_items(
            sub,
            TimestampsToReturn::Both,
            vec![MonitoredItemCreateRequest::from(NodeId::new(
                ns,
                format!("Machines/{mid}/EdgeAgent/PendingJobId"),
            ))],
        )
        .await
        .map_err(|e| anyhow::anyhow!("create_monitored_items: {e:?}"))?;
    tracing::info!(machine = %mid, "subscribed — waiting for jobs from gateway");

    loop {
        notify.notified().await;
        let Some(job_id) = pending.lock().unwrap().take() else {
            continue;
        };

        let csv = read_string(&session, &pending_csv_node).await.unwrap_or_default();
        tracing::info!(job = %job_id, bytes = csv.len(), "running job");

        let job = JobOrder::with_payload(job_id.clone(), "CutListCsv", csv.into_bytes());
        if let Err(e) = driver.run_job(&job).await {
            tracing::error!(job = %job_id, "driver failed: {e}");
            continue; // leave it published; gateway can re-dispatch / operator intervenes
        }

        match session
            .call_one((
                receiver_node.clone(),
                report_node.clone(),
                Some(vec![Variant::String(job_id.clone().into())]),
            ))
            .await
        {
            Ok(_) => tracing::info!(job = %job_id, "reported complete ✓"),
            Err(e) => tracing::warn!(job = %job_id, "ReportComplete failed: {e:?}"),
        }
    }
}

/// Read a String variable's value.
async fn read_string(session: &opcua::client::Session, node: &NodeId) -> Option<String> {
    let res = session
        .read(
            &[ReadValueId {
                node_id: node.clone(),
                attribute_id: AttributeId::Value as u32,
                ..Default::default()
            }],
            TimestampsToReturn::Both,
            0.0,
        )
        .await
        .ok()?;
    match &res.first()?.value {
        Some(Variant::String(s)) => s.value().clone(),
        _ => None,
    }
}

/// Resolve our namespace index from the server's namespace array.
async fn resolve_ns(session: &opcua::client::Session, uri: &str) -> Option<u16> {
    let res = session
        .read(
            &[ReadValueId {
                node_id: VariableId::Server_NamespaceArray.into(),
                attribute_id: AttributeId::Value as u32,
                ..Default::default()
            }],
            TimestampsToReturn::Server,
            0.0,
        )
        .await
        .ok()?;
    if let Some(Variant::Array(arr)) = &res.first()?.value {
        arr.values.iter().enumerate().find_map(|(i, v)| match v {
            Variant::String(s) if s.value().as_deref() == Some(uri) => Some(i as u16),
            _ => None,
        })
    } else {
        None
    }
}
