# Host window ↔ cylinder keybind (design)

## Goal

A keybind that moves the **focused host window** into the cylinder as its own panel, and back.

## What exists today (measured)

`host-mirror` uses xdg-desktop-portal **ScreenCast of a whole output** (monitor), not an arbitrary toplevel. There is no per-window capture path in this monorepo.

## Smallest real version (shipped)

`scripts/stardust-cylinder-toggle-mirror <eDP-1|Meta-0>` launches or stops a labeled whole-monitor mirror on the cylinder Wayland display via `stardust-cylinder-launch`. Bind in GNOME:

```
stardust-cylinder-toggle-mirror eDP-1
stardust-cylinder-toggle-mirror Meta-0
```

That is **monitor in/out**, not focused-window in/out.

## Feasible next step (not tonight)

1. Portal `ScreenCast` with window source (where the portal/compositor supports it), or
2. A small Wayland screencopy client aimed at a chosen toplevel (`wlr-screencopy` / ext-image-copy-capture), then the same `host-mirror`-style uinput path.

Until then, treat focused-window teleport as **infeasible with current host-mirror** and use monitor toggles + hexagon desktop entries.
