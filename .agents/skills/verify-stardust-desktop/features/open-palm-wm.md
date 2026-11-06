# Open-palm window management + reach ray

## Behaviour
- **Open palm** aims a visible reach ray; Hand → panel mouse is **suppressed** (pinch still clicks).
- **Fist** grabs whatever the ray hits: panel (Slide/Settle IPC) or cylinder bead (GroupGrab at distance).
- **Open** again releases / settles.
- Hysteresis on openness; pinch aborts reach and restores click.

## Drive without headset
```bash
scripts/verify/palm-reach-regression.sh
# or:
cargo test -p cylinder-shell --lib palm_wm
cargo test -p cylinder-shell --lib sim_hands
```

## Headset check
Open palm toward a mirror: ray visible, mouse must NOT move. Fist: panel slides. Aim at distant bead (even “inside desk”): fist moves cylinder. Pinch still clicks.
