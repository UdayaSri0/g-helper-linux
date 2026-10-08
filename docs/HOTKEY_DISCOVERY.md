# Hotkeys and ergonomic controls

## Design

ROG Helper does not capture keyboard input, open `/dev/input/event*`, grab devices, install a root hook, or register desktop-global shortcuts. Desktop environments own global shortcut mappings. Users may bind the semantic `rog-helper` commands below using their desktop's shortcut settings. Those commands call the session daemon; they never write hardware from the CLI process.

The implementation deliberately excludes GPU switching and page-specific UI activation. GPU mode changes can disrupt a session, while the UI currently does not export a stable page activation action. Both remain available through existing deliberate UI workflows.

## Linux input research

The Linux ASUS WMI driver exposes ASUS hotkey events through the standard input subsystem and maps recognized WMI events to standard key codes. This is not evidence that a specific physical key is mapped or emits on every model. On the development host, `/proc/bus/input/devices` showed an `Asus WMI hotkeys` input device, but no physical key sequence was captured or tested. ROG Helper does not consume that input device.

The desktop observed during implementation was Cinnamon/X11. Shortcut ownership and available registration APIs vary by desktop. In particular, ROG Helper does not assume that an XDG GlobalShortcuts portal is available, and it does not claim to own or discover the user's mappings.

## Commands available for desktop binding

| Action | Command | Behavior |
| --- | --- | --- |
| Cycle performance profile | `rog-helper profile cycle` | Uses only the profile order reported by asusd; requests the next profile through `rog-helperd` |
| Keyboard brightness up | `rog-helper lighting brightness up` | Reads current/max capability and requests a clamped one-step change through `rog-helperd` |
| Keyboard brightness down | `rog-helper lighting brightness down` | Reads current state and requests a clamped one-step change through `rog-helperd` |
| Toggle keyboard brightness | `rog-helper lighting brightness toggle` | Requests zero or the reported maximum through `rog-helperd` |
| Pause automation | `rog-helper automation pause` | Persists manual override until explicit resume |
| Resume automation | `rog-helper automation resume` | Clears manual override and reevaluates policy on the next sample |

If `rog-helperd` is unavailable, the CLI returns an actionable error. Unsupported profile or brightness capability also returns an error; the CLI does not guess profile names or silently fall back to raw sysfs.

On Cinnamon, configure these under **System Settings → Keyboard → Shortcuts → Custom Shortcuts**. On another desktop, use its keyboard-shortcut settings and bind the command as a custom command. The Settings page shows these commands and explains that mappings remain desktop-owned and undiscoverable to ROG Helper.

## References

- Linux input event codes: <https://docs.kernel.org/input/event-codes.html>
- ASUS WMI hotkey mapping in the Linux kernel: <https://github.com/torvalds/linux/blob/master/drivers/platform/x86/asus-nb-wmi.c>
- XDG Desktop Portal GlobalShortcuts interface (availability depends on the desktop portal implementation): <https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html>
