# Overnight generalize — staged installs

Binaries/scripts may be copied to `~/.local/share/roadie-backups/overnight-<ts>/` and optionally installed to `~/.local/bin` **without** restarting stardust-session.

Live `cylinder-shell` workers keep the old code in memory until mirrors are REPLACE’d or the session restarts.

Protostar hexagon fork is **not** installed live by default — build from `~/dev/open-source/protostar-cylinder` and swap `hexagon_launcher` only with Taylor OK + likely session restart.
