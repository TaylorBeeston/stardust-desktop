# stardust-xr-server-verify

Patched Stardust server for agent headless verification. **Do not overwrite** the live
`stardust-bootstrap/prefix/usr/bin/stardust-xr-server` used by Taylor's session.

## What it adds

- `VerifyShotPlugin`: when `CYLVERIFY_SHOT_REQUEST=/path/frame.png` is set, creating
  `/path/frame.png.request` captures the primary flatscreen window to that PNG
  (in-engine; works under NVIDIA+Xvfb where `ffmpeg x11grab` is black).
- `CYLVERIFY_CAM_YAW_DEG` / `CYLVERIFY_CAM_PITCH_DEG` / `CYLVERIFY_CAM_POS=x,y,z`
  set the initial flatscreen camera.

## Build / install (FrenchFry)

```bash
cd ~/dev/open-source/stardust-bootstrap/org.stardustxr.Server
# apply staging/stardust-server-verify/server-verify.patch and add verify_shot.rs
# OR use the already-patched tree if present
cargo build --release -p stardust-xr-server
install -Dm755 target/release/stardust-xr-server ~/.local/bin/stardust-xr-server-verify
```

`scripts/verify/headless-vr.sh` prefers `~/.local/bin/stardust-xr-server-verify`.
