# Cylinder headset side test

Run cylinder beside a live session **without** restarting Stardust / WiVRn / GNOME.

Production cutover is documented in `docs/cylinder-cutover.md`. Use this side path to
validate a new cylinder-shell build before promoting it.

## Preconditions

- Live `stardust-session` with a working server socket (`stardust-0`).
- `just install-cylinder-shell` (or equivalent) so `cylinder-shell` / `cylinder-layout` are on
  `PATH` / `$PREFIX`.
- Do not restart WiVRn, WayVR, or GNOME for this test.
- Do not replace the WaylandService binary as part of this test.

## Go-live (side)

```sh
scripts/cyl-headset-side-start
scripts/cyl-headset-side-launch-app          # ghostty default
# optional:
# scripts/cyl-headset-side-launch-app kitty
# scripts/cyl-headset-side-launch-mirror
scripts/stardust-cylinder-nudge radius 0.70
```

Stop (live Flatland / production cylinder untouched if you only used the side sock):

```sh
scripts/cyl-headset-side-stop
```

## What to check in headset

1. Concave face readable from inside the cylinder.
2. Pointer / click lands on the texel the eye sees.
3. Multi-app pack with gap ahead; no `app_id` config required for Ghostty-class apps.
4. Group move via IPC: `scripts/stardust-cylinder-nudge grab 0.15 0` (visual bead not shipped).

## Risks

- Side wayland-service shares the live server (extra GPU/CPU). Stop when done.
- No visual group-grab bead yet.
- Touch / child surfaces / popups incomplete on cylinder.
