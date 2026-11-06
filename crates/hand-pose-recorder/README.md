# hand-pose-recorder

Standalone Stardust client for labeled hand-pose capture (no session restart).

## Launch (live session)

```bash
STARDUST_INSTANCE=stardust-0 \
STARDUST_RES_PREFIXES="$HOME/.local/share:$HOME/dev/open-source/stardust-bootstrap/prefix/usr/share:/usr/share" \
~/.local/bin/hand-pose-recorder
```

Panel reparents to `stardust-hmd` (~0.55 m forward, slightly below eyes).

## Use

1. Pinch a label button with either hand (other hand can hold the pose).
2. Hold the named pose through a **3 s countdown**.
3. Stay still for **3 s recording** (badge turns red).
4. Green **DONE** — sample written under `~/.local/state/hand-pose-recorder/`.

Labels: Open palm (aimed at panel), Palm facing me, Back of hand, Relaxed, Fist, Loose fist, Pinch, Point.

Each JSONL frame includes both hands' full joints, derived palm_normal/distal/tip distances, and best-effort head Tip/Pointer.
