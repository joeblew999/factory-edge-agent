# factory-edge-agent

<https://github.com/joeblew999/factory-edge-agent>

Runs a machine driver **at the edge** — on the box physically wired to the machine
(e.g. a Raspberry Pi plugged into a Howick FRAMA) — and connects to
[factory-gateway](https://github.com/joeblew999/factory-gateway) over OPC-UA.

This is the **real factory-floor topology**: machines are spread across the floor,
so each runs its own agent process that talks back to one central per-factory
gateway. (For a single co-located machine you can instead run the driver
in-process in the gateway — set `edge = false` on that machine. For anything
distributed, you want this.)

Part of the `factory-` family:

| Repo | Role |
|------|------|
| [factory-machine-model](https://github.com/joeblew999/factory-machine-model) | the contract |
| [factory-gateway](https://github.com/joeblew999/factory-gateway) | the OPC-UA server this agent connects to |
| [factory-howick-driver](https://github.com/joeblew999/factory-howick-driver) | the driver this agent hosts |
| **factory-edge-agent** (this) | runs the driver at the machine, over OPC-UA |
| [factory-floor](https://github.com/joeblew999/factory-floor) | umbrella + docs |

## How it works (OPC-UA, subscribe-don't-poll)

The agent is an OPC-UA **client** of the gateway. It uses the standard
SCADA-to-device pattern — subscribe to a node, get pushed updates:

```text
gateway  Machines/<id>/EdgeAgent/PendingJobId   ──push──▶  agent subscribes
         Machines/<id>/EdgeAgent/PendingJobCsv   ──read──▶  agent reads the cut-list
                                                            agent runs the driver → writes to the machine
         Machines/<id>/JobOrderReceiver/ReportComplete(JobOrderID) ◀──call── agent reports done
```

1. Connect to the gateway, resolve the namespace.
2. **Subscribe** to `PendingJobId` — the gateway pushes the instant a job is
   dispatched to this machine.
3. Read `PendingJobCsv`, run the local driver (the Howick driver writes the
   cut-list to the FRAMA's USB), then call the ISA-95 `ReportComplete` method.

No polling. No custom protocol. The same pattern SCADA uses against Siemens PLCs
or Fanuc CNCs.

## Run it

```bash
factory-edge-agent --config examples/agent.toml
```

## Verified end-to-end

`cargo test` (in-crate `src/e2e.rs`) starts a **real gateway** and a **real edge
agent**, dispatches a job to the gateway, and asserts the cut-list arrives at the
machine — written by the agent after the gateway published it over OPC-UA — and
that the gateway marks the job complete once the agent calls `ReportComplete`.
The full distributed path is exercised on the wire.

## Licence

MIT OR Apache-2.0.
