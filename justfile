# stardust-desktop: cylinder-shell + host-mirror + desk toys
prefix := env_var_or_default("PREFIX", env_var("HOME") / ".local")

default: build

build:
    cargo build --release --workspace

test:
    cargo test --release --workspace

# Install binaries, desk-toy models, helper scripts and the panel shell into {{prefix}}.
# Models go to {{prefix}}/share/io.github.TaylorBeeston.DeskToys: add {{prefix}}/share to STARDUST_RES_PREFIXES.
install: build
    install -Dm755 -t {{prefix}}/bin target/release/host-mirror target/release/wrist-menu target/release/newtons-cradle target/release/spin-sign
    install -Dm755 -t {{prefix}}/bin scripts/stardust-dual-mirror-flat scripts/stardust-dual-mirror-recenter scripts/stardust-recenter-all scripts/stardust-cylinder-launch scripts/stardust-cylinder-nudge scripts/stardust-cylinder-toggle-mirror
    install -Dm644 -t {{prefix}}/share/applications share/applications/*.desktop
    install -d {{prefix}}/share/io.github.TaylorBeeston.DeskToys {{prefix}}/libexec
    install -m644 crates/desk-toys/data/io.github.TaylorBeeston.DeskToys/*.glb {{prefix}}/share/io.github.TaylorBeeston.DeskToys/
    sed 's|@PREFIX@|{{prefix}}|g' scripts/stardust_panel_shell.in > {{prefix}}/libexec/stardust_panel_shell
    chmod 755 {{prefix}}/libexec/stardust_panel_shell
    @echo "panel shell: stardust-xr-wayland-service -p {{prefix}}/libexec/stardust_panel_shell"

# Sample configs (won't overwrite existing ones)
install-config:
    install -d ~/.config/stardust-desktop ~/.config/io.github.TaylorBeeston.WristMenu
    cp -n examples/panels.conf ~/.config/stardust-desktop/panels.conf || true
    cp -n examples/items.txt ~/.config/io.github.TaylorBeeston.WristMenu/items.txt || true

# Headless checks: build, tests, script syntax, a throwaway install into a temp prefix
verify:
    scripts/verify.sh


# Cylinder panel shell + layout daemon (live product -p). Also keep `just install` for
# host-mirror, desk toys, and the legacy Flatland wrapper used by rollback.
install-cylinder-shell: build
    install -Dm755 -t {{prefix}}/bin target/release/cylinder-shell target/release/cylinder-layout scripts/stardust-cylinder-nudge scripts/stardust-cylinder-launch
    install -Dm755 -t {{prefix}}/bin scripts/stardust-cylinder-nudge scripts/stardust-mirrors-autostart
    install -Dm755 -t {{prefix}}/bin scripts/cyl-headset-side-start scripts/cyl-headset-side-stop scripts/cyl-headset-side-launch-app scripts/cyl-headset-side-launch-mirror
    install -d {{prefix}}/libexec
    sed 's|@PREFIX@|{{prefix}}|g' scripts/stardust_cylinder_shell.in > {{prefix}}/libexec/stardust_cylinder_shell
    chmod 755 {{prefix}}/libexec/stardust_cylinder_shell
    @echo "cylinder-shell + cylinder-layout + libexec/stardust_cylinder_shell installed. Point -p at that wrapper (see docs/cylinder-cutover.md)."
