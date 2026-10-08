# Display and Power Convenience Discovery

## Findings for the inspected session

The inspected desktop session is Cinnamon on X11 (`XDG_SESSION_TYPE=x11`). `/usr/bin/xrandr`
is installed and its read-only query reports a connected internal eDP panel, current mode
1920x1200 at 165 Hz, and 60 Hz as another rate for that same mode. A separate HDMI output is
connected. The panel output name is discovered from the live RandR response; it is not stored
or hard-coded. The DRM sysfs connector and RandR output names differ on this host, so they are
not joined by a guessed monitor name.

The shared `rog-providers::display` backend implements manual X11 refresh selection, invoked by
the UI in its active desktop session. It offers rates advertised for the
currently active internal-panel resolution, re-discovers before applying, runs `xrandr` with
fixed argument positions (no shell), reads the active rate back, and attempts to restore the
previous rate when the command fails or the requested mode is not confirmed. It does not
change resolution, external outputs, or saved profiles. Apply is unavailable if XRandR cannot
be queried, no single connected internal panel can be identified, or the current mode/rate is
not reported.

Wayland remains unsupported. The inspected Cinnamon session exports a Muffin DisplayConfig
DBus API, but this is a compositor-specific configuration contract rather than a portable
Wayland protocol. It is not used as a fallback. XRandR is an X11 RandR client; its manual
documents selecting an output, mode, and rate, and notes that `--rate` selects the closest
rate for duplicate modes. This implementation narrows that behavior to rates parsed from the
live current-mode list. See the [XRandR manual](https://xorg.freedesktop.org/archive/X11R7.5/doc/man/man1/xrandr.1.html)
and [Cinnamon Muffin DisplayConfig interface](https://github.com/linuxmint/muffin/blob/master/src/org.cinnamon.Muffin.DisplayConfig.xml).

## Other candidates

- **AC/Battery refresh rule:** not implemented. The display controller is owned by the active
  desktop session, while `rog-helperd` is a separate user service that does not receive a
  guaranteed `DISPLAY` or session-type environment. Adding a second watcher or guessing the
  active display session would violate the shared Prompt 06 policy design. A future rule needs
  a verified session-bound control contract and must use the existing policy engine.
- **Keyboard backlight timeout:** not implemented. The inspected ASUS WMI LED exposes
  `brightness`, `max_brightness`, and trigger attributes, but no timeout attribute. Cinnamon
  exposes idle dim/screen-sleep preferences for its power manager, not a keyboard-specific
  timeout API. Software dimming is intentionally omitted to avoid competing with that manager.
- **Panel power saving:** not implemented. Cinnamon's power manager already owns idle dim and
  display-sleep policy. Duplicating those settings in ROG Helper could conflict with desktop
  policy and AC/Battery behavior.
- **Display visibility/diagnostics:** the Settings section reports session/backend, discovered
  internal panel, current mode/rate, and the reason control is unavailable. No display state is
  presented as daemon hardware telemetry.

The kernel DRM connector model exposes connector status and modes to display userspace; it
does not provide a portable session-level mode-setting API to an ordinary application. See
the [Linux DRM/KMS documentation](https://www.kernel.org/doc/html/latest/gpu/drm-kms.html).

## Validation boundary

Unit tests cover output/rate parsing, current-mode scoping, unsupported Wayland and
external-only cases, rejection of unadvertised rates, and rollback after a failed mode change.
The inspected host's `xrandr --query` was run read-only. No physical refresh-rate change was
attempted; the implementation is not a claim of target-model physical validation. Mode changes
remain explicitly user initiated and are never applied by Prompt 06 automation.
