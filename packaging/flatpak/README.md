# Flatpak Packaging Notes

This directory contains a local Flatpak manifest and dependency manifest for
`rog-helper`. It is structured to stay close to Flathub conventions without
claiming that a Flathub submission is already live.

## Files

- `io.github.roghelper.UI.yml`
- `cargo-sources.json`
- `flathub.json`

## Local build

From a checked-out repository:

```bash
flatpak remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
packaging/scripts/build-flatpak.sh
flatpak install --user --reinstall dist/flatpak-repo io.github.roghelper.UI
flatpak run io.github.roghelper.UI
```

The Flatpak runs `rog-helper-ui` inside the sandbox and installs
`/app/share/dbus-1/services/io.github.roghelper.Daemon.service` so the UI can
activate the bundled `rog-helperd` on demand.

## Permissions

The manifest intentionally requests only the Flatpak permissions needed by the
current codebase:

- `--socket=wayland`, `--socket=fallback-x11`, `--share=ipc`, `--device=dri`
  for GTK rendering
- `--share=network` for the existing GitHub release check in the About page
- `--talk-name=io.github.roghelper.Daemon` and
  `--own-name=io.github.roghelper.Daemon` so the UI can reach the bundled
  session-bus daemon
- `--system-talk-name=org.freedesktop.UPower`,
  `--system-talk-name=org.asuslinux.Daemon`, and
  `--system-talk-name=org.supergfxctl.Daemon` for the host services already
  used by the provider layer

## Flatpak limitations

The Flatpak build is intentionally honest about sandbox tradeoffs:

- the host `systemd --user` unit is not installed or enabled from Flatpak
- direct sysfs writes are not available, so CPU tuning, per-core toggles, and
  keyboard-backlight writes remain unavailable or read-only
- `asusd` and `supergfxd` access still depends on the host services being
  installed and allowing the requested D-Bus calls from the sandbox
- procfs-backed per-process diagnostics are sandbox-scoped rather than a full
  host process view
- the `nvidia-smi` fallback is not expected to work inside the sandbox because
  the host binary is not bundled
- tray integration may still be limited on desktops that require extra
  StatusNotifier D-Bus ownership beyond the permissions granted here

For the fullest feature set, native host packages are still the recommended
install path.

## Updating cargo sources

If `Cargo.lock` changes, refresh `cargo-sources.json` with a compatible
`flatpak-cargo-generator` installation before rebuilding the Flatpak metadata.

## Future Flathub submission

This repository does not claim Flathub publication today. A future submission
would still need:

- a published source archive or tag-backed source entry instead of the local
  `type: dir` source used here
- the Flatpak manifest and `flathub.json` moved to the repository root for a
  Flathub submission pull request
- review of the custom D-Bus permissions and the sandbox limitations above
