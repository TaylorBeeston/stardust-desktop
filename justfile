# stardust-desktop: host-mirror + desk toys + panel-shell helpers
prefix := env_var_or_default("PREFIX", env_var("HOME") / ".local")

default: build

build:
    cargo build --release --workspace

test:
    cargo test --release --workspace

# Install binaries, desk-toy models, helper scripts and the panel shell into {{prefix}}.
# Models go to {{prefix}}/share/org.stardustxr.DeskToys: add {{prefix}}/share to STARDUST_RES_PREFIXES.
install: build
    install -Dm755 -t {{prefix}}/bin target/release/host-mirror target/release/wrist-menu target/release/newtons-cradle target/release/spin-sign
    install -Dm755 -t {{prefix}}/bin scripts/stardust-dual-mirror-flat scripts/stardust-dual-mirror-recenter scripts/stardust-recenter-all
    install -d {{prefix}}/share/org.stardustxr.DeskToys {{prefix}}/libexec
    install -m644 crates/desk-toys/data/org.stardustxr.DeskToys/*.glb {{prefix}}/share/org.stardustxr.DeskToys/
    sed 's|@PREFIX@|{{prefix}}|g' scripts/stardust_panel_shell.in > {{prefix}}/libexec/stardust_panel_shell
    chmod 755 {{prefix}}/libexec/stardust_panel_shell
    @echo "panel shell: stardust-xr-wayland-service -p {{prefix}}/libexec/stardust_panel_shell"

# Sample configs (won't overwrite existing ones)
install-config:
    install -d ~/.config/stardust-desktop ~/.config/org.stardustxr.WristMenu
    cp -n examples/panels.conf ~/.config/stardust-desktop/panels.conf || true
    cp -n examples/items.txt ~/.config/org.stardustxr.WristMenu/items.txt || true

# Headless checks: build, tests, script syntax, a throwaway install into a temp prefix
verify:
    scripts/verify.sh


# Scaffold only: build cylinder-shell binaries into {{prefix}}/bin. Does NOT replace
# the Flatland-backed stardust_panel_shell until the cylinder worker accepts panels.
install-cylinder-shell: build
    install -Dm755 -t {{prefix}}/bin target/release/cylinder-shell target/release/cylinder-layout
    @echo "cylinder-shell scaffold installed; point -p at {{prefix}}/bin/cylinder-shell only after PanelItemAcceptor is wired (see docs/cylinder-shell-design.md)"
