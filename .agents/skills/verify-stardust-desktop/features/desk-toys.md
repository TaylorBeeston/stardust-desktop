# Desk toys / wrist menu

`wrist-menu`, `newtons-cradle`, `spin-sign` Stardust clients.

## Sub-features

- `wrist-toggle` — tap row to start/stop.
- `place-here` — right strip respawns at head via fresh startup token.
- `cradle-physics` / `spin-sign` — inertia and slap behaviour (unit-tested).

## How to get to it (user POV)

- Wrist menu on left wrist in session; items from `~/.config/org.stardustxr.WristMenu/items.txt`.

## Driving it with cargo test

Preconditions:

- Workspace builds.

- **Units.** `cargo test -p desk-toys`.
- XR interaction is headset-only.

## Gotchas

- Place-here is kill+respawn, not relocate IPC.
