# Developer Installation

Run the production development installer as your normal desktop user:

```bash
./packaging/scripts/install-dev.sh
```

The script builds the release binaries and final Debian package, shows the exact package it
will install, and invokes `sudo apt install` once. It then returns to the original user context,
reloads that user's systemd manager, restarts an already-running `rog-helperd`, and runs both
`rog-helper privileged-status` and `rog-helper lighting-diagnostics`.

Do not invoke the script with `sudo`. The UI, CLI, and session daemon are user processes. Only
APT needs administrator access to install the root-owned helper and system integration.

Package validation can be repeated explicitly with:

```bash
./packaging/scripts/check-deb-package.sh dist/rog-helper_VERSION_ARCH.deb
```

This checks package identity and relationships, the complete payload, file ownership and modes,
template substitution, maintainer-script safety, desktop and AppStream metadata, systemd units,
D-Bus and PolicyKit XML, the narrow Aura udev rule, and privileged-helper hardening.
