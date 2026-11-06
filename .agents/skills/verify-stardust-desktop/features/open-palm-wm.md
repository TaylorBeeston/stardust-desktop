# Open-palm window management + reach ray

## Fitted behaviour (2026-11-06 labeled takes)
- Ray = protocol **palm_normal (−Y)**, blended toward distal; not +Y (back of hand).
- Arm only when: fingers extended (`mean≥0.112`, `min≥0.092`), palm faces content (`n·toward≥0.50`),
  distal not too up (`distal.y≤0.915`, rejects Back of hand), held ~150 ms.
- Fist/Loose fist: `mean_tip≤0.062` **before** pinch check; Pinch/Relaxed stay click/idle.
- Pinch still clicks when not fist-curled.

## Drive without headset
```bash
scripts/verify/palm-reach-regression.sh
# fixtures: crates/cylinder-shell/tests/fixtures/hand_poses/
```

## Capture more labels
```bash
STARDUST_INSTANCE=stardust-0 ~/.local/bin/hand-pose-recorder
# → ~/.local/state/hand-pose-recorder/
```
