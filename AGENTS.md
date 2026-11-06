# stardust-desktop — agent notes

## Verification

Before claiming cylinder / mirror / desk-toy work is done, run the project verification skill:

- Skill: `.agents/skills/verify-stardust-desktop/` (also linked from `.cursor/skills/`)
- Quick path: `scripts/verify/doctor.sh && scripts/verify/headless-vr.sh (preferred visual)
- scripts/verify/ipc-scenarios.sh && cargo test --workspace`
- Full: `scripts/verify.sh`
- **Never** drive the live `stardust-cylinder/layout.sock` or restart stardust-session / WiVRn / WayVR / GNOME unless the user explicitly approves that restart.

## Push rules

- Fast-forward push to `main` only on `TaylorBeeston/stardust-desktop` after verification passes.
- No force-push. Prefer `roadie/*` worktree branches.
