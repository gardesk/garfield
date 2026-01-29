# garfield-portal

XDG Desktop Portal backend for garfield file picker.

This daemon implements the `org.freedesktop.impl.portal.FileChooser` interface, allowing applications to use garfield as the system file picker dialog.

## How It Works

When an application (GTK, Qt, Flatpak, etc.) requests a file dialog through the portal API:

1. `xdg-desktop-portal` receives the request
2. It routes the request to `garfield-portal` based on configuration
3. `garfield-portal` spawns `garfield --picker` with appropriate options
4. User selects files in garfield
5. Selected paths are returned to the application as `file://` URIs

## Building

```bash
cargo build --release -p garfield-portal
```

## Installation

### System-wide (recommended)

```bash
cd garfield-portal
./install.sh
```

This installs:
- Binary to `/usr/local/bin/garfield-portal`
- Portal config to `/usr/share/xdg-desktop-portal/portals/garfield.portal`
- Systemd service to `/usr/lib/systemd/user/garfield-portal.service`
- Portal preferences to `/usr/share/xdg-desktop-portal/gar-portals.conf`

### User-only

```bash
./install.sh --user
```

Installs to `~/.local/` directories instead.

### Enable the Service

After installation:

```bash
systemctl --user daemon-reload
systemctl --user enable --now garfield-portal
```

## Testing

Run the test script:

```bash
./test-portal.sh
```

Or test with real applications:

```bash
# GTK apps
GTK_USE_PORTAL=1 gedit
GTK_USE_PORTAL=1 firefox

# Qt/KDE apps
QT_QPA_PLATFORMTHEME=xdgdesktopportal dolphin

# Flatpak apps (automatically use portals)
flatpak run org.gnome.TextEditor
```

## Configuration

### Making garfield the default

For the gar desktop environment, the installer creates `gar-portals.conf`:

```ini
[preferred]
default=garfield;gtk
org.freedesktop.impl.portal.FileChooser=garfield
```

Set `XDG_CURRENT_DESKTOP=gar` in your session to activate this configuration.

### Manual configuration

To use garfield in other desktop environments, create `~/.config/xdg-desktop-portal/portals.conf`:

```ini
[preferred]
default=garfield;gtk
```

Then restart xdg-desktop-portal:

```bash
systemctl --user restart xdg-desktop-portal
```

## Uninstallation

```bash
./install.sh --uninstall      # System-wide
./install.sh --uninstall-user # User-only
```

## Troubleshooting

### Check installation status

```bash
./install.sh --check
```

### View logs

```bash
journalctl --user -u garfield-portal -f
```

### Common issues

**"Portal not found"**: Make sure `garfield-portal` is running:
```bash
systemctl --user status garfield-portal
```

**"garfield not found"**: The `garfield` binary must be in PATH for picker mode to work.

**Dialog doesn't appear**: Check if `DISPLAY` is set and garfield can connect to X11.

## D-Bus Interface

The portal implements `org.freedesktop.impl.portal.FileChooser`:

- `OpenFile(handle, app_id, parent_window, title, options)` - Open file dialog
- `SaveFile(handle, app_id, parent_window, title, options)` - Save file dialog
- `SaveFiles(handle, app_id, parent_window, title, options)` - Save multiple files

See the [XDG Desktop Portal documentation](https://flatpak.github.io/xdg-desktop-portal/) for full details.
