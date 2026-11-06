# Cylinder layout IPC

The `cylinder-layout` daemon owns shared cylinder state and assigns slots to panel workers over a Unix socket.

## Sub-features

- `hello-pack` — Hello assigns alternating right/left yaw with a gap ahead.
- `group-grab` — GroupGrab yaw/height broadcasts GroupChanged to all workers.
- `set-radius` — SetRadius reclamps and re-packs.
- `metrics-changed` — MetricsChanged updates size and re-packs.
- `bye-repack` — Bye or disconnect releases a slot and re-packs neighbours.
- `reanchor` — Reanchor puts the gap ahead of the given HMD yaw/height.

## How to get to it (user POV)

- Session startup starts `cylinder-layout` then workers Hello on accept.
- `stardust-cylinder-nudge grab|radius|reanchor` sends group controls.
- Workers send MetricsChanged when a surface resizes (when wired).

## Driving it with ipc-scenarios

Preconditions:

- `scripts/verify/doctor.sh` exits 0.
- No reliance on the live layout sock.

- **Run suite.** `scripts/verify/ipc-scenarios.sh`. Exit 0; `.verify-evidence/ipc-*/results.json` lists every scenario with `"ok": true`.
- **Packing proof.** `hello_pack_rl` shows first yaw > 0 and second yaw < 0.
- **Grab proof.** After GroupGrab, GroupChanged height/yaw match the request.
- **Radius proof.** SetRadius 0.90 appears in GroupChanged.
- **Bye proof.** After Bye of the left panel, the remaining slot yaw is > 0.

## Gotchas

- Live `layout.sock` must never be the target; the script always uses a temp sock.
- Same-process flock tests are not a substitute for this daemon drive.
- Overflow scaling can shrink `width_m` below natural width at small radii — assert intent, not raw px/density.
