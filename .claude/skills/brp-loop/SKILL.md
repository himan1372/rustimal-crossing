---
name: brp-loop
description: Drive the running Bevy game through the Bevy Remote Protocol (BRP) at localhost:15702 using the observe -> decide -> act -> verify agent loop. Use for inspecting the live world (entities, components, resources), mutating game state, sending input, and capturing screenshots.
---

# BRP Agent Loop

Drive the running Bevy game through the Bevy Remote Protocol (BRP) HTTP server.
The loop is: **observe → decide → act → verify**. Never skip a step.

## Which app serves BRP

The BRP-enabled Bevy 0.19 app lives in the sibling repo **himan1372/bevy-crossing**
(`app/`), not in this repo. This repo's `town_prototype/` and `showcase/` are
plain-Rust CLIs with no BRP.

The bevy-crossing wiring (verified on its `main` branch):

- `app/Cargo.toml`: `bevy = "0.19"`, optional `bevy_brp_extras = "0.22.1"`.
- Features: `remote = ["dep:bevy_brp_extras", "bevy/bevy_remote"]` enables the
  BRP HTTP server on port 15702. `capture = ["remote"]` adds screenshot /
  diagnostics extras.
- `app/src/remote.rs`: `RemotePluginGate` adds
  `bevy_brp_extras::BrpExtrasPlugin::default()` when the `remote` feature is on.
  Purely additive — default builds do nothing.
- Launch it: `cd bevy-crossing/app && cargo run --features remote`
  (or `--features capture`).

`BrpExtrasPlugin` registers extra BRP methods (`brp_extras/screenshot`,
`brp_extras/shutdown`, `brp_extras/send_keys`, `brp_extras/set_window_title`,
`brp_extras/get_diagnostics`, `brp_extras/agent_tools`, …) and, on native
targets, adds `bevy::remote::http::RemoteHttpPlugin` — the HTTP transport —
automatically. The raw Bevy 0.19 equivalent is adding `RemotePlugin` (core
protocol) plus `RemoteHttpPlugin` (HTTP on 15702) yourself, with the
`bevy/bevy_remote` cargo feature compiling the `bevy::remote` module in.

Port default is 15702, overridable with the `BRP_EXTRAS_PORT` env var.
Screenshots need bevy's `png` feature enabled.

## Protocol

HTTP POST to `http://localhost:15702` with a JSON-RPC 2.0 body:

```json
{"jsonrpc": "2.0", "id": 1, "method": "bevy/query", "params": {...}}
```

- `method` is `bevy/<name>` (built-in) or `brp_extras/<name>` (extras).
- Component/resource types are named by full Rust path, e.g.
  `"bevy_transform::components::transform::Transform"`.
- Entities are integer IDs — always use values returned by queries, never guess.
- `brp_extras/screenshot` is a *watching* method: the HTTP call blocks until the
  PNG is written. Give it a generous timeout (30s+).

## The loop

### 1. OBSERVE

Query the world before touching it.

```bash
# List entities with a Transform, returning Transform + Name (Name optional)
curl -s -X POST http://localhost:15702 -H 'Content-Type: application/json' -d '{
  "jsonrpc": "2.0", "id": 1, "method": "bevy/query",
  "params": {"data": {"components": [
    "bevy_transform::components::transform::Transform",
    "bevy_ecs::name::Name"
  ], "option": ["bevy_ecs::name::Name"]}}}
}' | python3 -m json.tool

# Read one entity's components
curl -s -X POST http://localhost:15702 -H 'Content-Type: application/json' -d '{
  "jsonrpc": "2.0", "id": 2, "method": "bevy/get",
  "params": {"entity": 12345, "components": [
    "bevy_transform::components::transform::Transform"
  ]}}
}'

# Read a resource
curl -s -X POST http://localhost:15702 -H 'Content-Type: application/json' -d '{
  "jsonrpc": "2.0", "id": 3, "method": "bevy/get_resource",
  "params": {"resource": "my_crate::GameState"}}
}'

# See what methods the server offers
curl -s -X POST http://localhost:15702 -H 'Content-Type: application/json' -d '{
  "jsonrpc": "2.0", "id": 4, "method": "rpc.discover", "params": {}}'

# Visual check: full primary-window screenshot (blocking — allow 30s+)
curl -s --max-time 60 -X POST http://localhost:15702 \
  -H 'Content-Type: application/json' -d '{
  "jsonrpc": "2.0", "id": 5, "method": "brp_extras/screenshot",
  "params": {"path": "/tmp/brp-check.png"}}'
```

### 2. DECIDE

Compare the observation against the goal. Choose the smallest mutation that moves
toward it. Prefer mutating an existing entity over spawning; prefer one field
change over a component swap.

### 3. ACT

Apply the mutation. Examples:

```bash
# Spawn an entity with components
curl -s -X POST http://localhost:15702 -H 'Content-Type: application/json' -d '{
  "jsonrpc": "2.0", "id": 6, "method": "bevy/spawn",
  "params": {"components": {
    "bevy_transform::components::transform::Transform": {
      "translation": [0.0, 1.0, 0.0],
      "rotation": [0.0, 0.0, 0.0, 1.0],
      "scale": [1.0, 1.0, 1.0]
    },
    "bevy_ecs::name::Name": {"name": "agent-marker"}
  }}}'

# Change one field of a component (path = field path inside the component)
curl -s -X POST http://localhost:15702 -H 'Content-Type: application/json' -d '{
  "jsonrpc": "2.0", "id": 7, "method": "bevy/mutate_component",
  "params": {"entity": 12345,
    "component": "bevy_transform::components::transform::Transform",
    "path": ".translation.x", "value": 5.0}}
}'

# Insert a component onto an existing entity
curl -s -X POST http://localhost:15702 -H 'Content-Type: application/json' -d '{
  "jsonrpc": "2.0", "id": 8, "method": "bevy/insert",
  "params": {"entity": 12345, "components": {
    "bevy_ecs::name::Name": {"name": "renamed-by-agent"}
  }}}'

# Remove a component
curl -s -X POST http://localhost:15702 -H 'Content-Type: application/json' -d '{
  "jsonrpc": "2.0", "id": 9, "method": "bevy/remove",
  "params": {"entity": 12345, "components": ["bevy_ecs::name::Name"]}}
}'

# Destroy an entity
curl -s -X POST http://localhost:15702 -H 'Content-Type: application/json' -d '{
  "jsonrpc": "2.0", "id": 10, "method": "bevy/destroy",
  "params": {"entity": 12345}}
}'

# Mutate a resource field
curl -s -X POST http://localhost:15702 -H 'Content-Type: application/json' -d '{
  "jsonrpc": "2.0", "id": 11, "method": "bevy/mutate_resource",
  "params": {"resource": "my_crate::GameState", "path": ".day", "value": 3}}
}'

# Drive gameplay input: press and release a key
# (exact schema via brp_extras/agent_tools — this is the common shape)
curl -s -X POST http://localhost:15702 -H 'Content-Type: application/json' -d '{
  "jsonrpc": "2.0", "id": 12, "method": "brp_extras/send_keys",
  "params": {"keys": [{"key": "KeyW", "duration_ms": 200}]}}'

# Screenshot cropped to one entity (needs its entity id; padding in px)
curl -s --max-time 60 -X POST http://localhost:15702 \
  -H 'Content-Type: application/json' -d '{
  "jsonrpc": "2.0", "id": 13, "method": "brp_extras/screenshot",
  "params": {"path": "/tmp/brp-entity.png", "entity": 12345, "padding": 8}}'
```

### 4. VERIFY

Never trust a write. Re-observe:

1. Re-run the `bevy/get` / `bevy/query` from step 1 and confirm the value changed.
2. Take a screenshot and look at it when the change is visual.
3. If the state doesn't match, go back to DECIDE — don't stack more mutations blind.

When done driving the app, shut it down cleanly:

```bash
curl -s -X POST http://localhost:15702 -H 'Content-Type: application/json' -d '{
  "jsonrpc": "2.0", "id": 99, "method": "brp_extras/shutdown", "params": {}}'
```

## Rules

1. **Check the server is up first.** If `curl` can't connect, the game isn't
   running with `--features remote` — start it, don't fake results.
2. **Observe before every act.** No blind mutations.
3. **Verify every write** with a re-query before moving on.
4. **One mutation at a time.** If verification fails, diagnose before acting again.
5. **Use exact type paths.** A wrong component path returns an error, not a
   guess — read the error and fix it.
6. **Don't spawn duplicates.** Query first; if the entity exists, mutate it.
7. **Watching methods block.** Give `brp_extras/screenshot` a long timeout;
   don't retry while one is in flight.
8. Keep each loop tight: one observation, one decision, one action, one
   verification.

## Quick reference

| Goal | Method |
|---|---|
| List entities/components | `bevy/query` |
| Read entity | `bevy/get` |
| Create entity | `bevy/spawn` |
| Delete entity | `bevy/destroy` |
| Add component | `bevy/insert` |
| Remove component | `bevy/remove` |
| Edit component field | `bevy/mutate_component` |
| Read resource | `bevy/get_resource` |
| Write resource | `bevy/insert_resource` / `bevy/mutate_resource` |
| List available methods | `rpc.discover` |
| Screenshot | `brp_extras/screenshot` |
| Key input | `brp_extras/send_keys` |
| Set window title | `brp_extras/set_window_title` |
| Diagnostics | `brp_extras/get_diagnostics` |
| Agent tool schemas | `brp_extras/agent_tools` |
| Shutdown | `brp_extras/shutdown` |

Port 15702 · JSON-RPC 2.0 over HTTP POST · Bevy 0.19 · bevy_brp_extras 0.22
