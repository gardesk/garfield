#!/bin/bash
# Install script for garfield-portal
#
# This script installs the garfield portal backend which allows
# applications to use garfield as the system file picker.
#
# Usage:
#   ./install.sh         - Install to system (requires sudo)
#   ./install.sh --user  - Install to user directory only
#   ./install.sh --check - Check installation status

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Directories
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINARY_NAME="garfield-portal"
GARFIELD_BINARY="garfield"
PORTAL_FILE="garfield.portal"
SERVICE_FILE="garfield-portal.service"

# System paths
SYS_BIN_DIR="/usr/local/bin"
SYS_PORTAL_DIR="/usr/share/xdg-desktop-portal/portals"
SYS_PORTALS_CONF_DIR="/usr/share/xdg-desktop-portal"

# User paths
USER_BIN_DIR="$HOME/.local/bin"
USER_PORTAL_DIR="$HOME/.local/share/xdg-desktop-portal/portals"
USER_SERVICE_DIR="$HOME/.config/systemd/user"
USER_PORTALS_CONF_DIR="$HOME/.config/xdg-desktop-portal"

print_status() {
    echo -e "${GREEN}[OK]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

check_binary() {
    local name="$1"
    local binary_path="$SCRIPT_DIR/../target/release/$name"
    if [[ ! -f "$binary_path" ]]; then
        binary_path="$SCRIPT_DIR/target/release/$name"
    fi
    if [[ ! -f "$binary_path" ]]; then
        # Try workspace root
        binary_path="$(dirname "$SCRIPT_DIR")/target/release/$name"
    fi
    if [[ ! -f "$binary_path" ]]; then
        print_error "Binary '$name' not found. Please run 'cargo build --release' first."
        exit 1
    fi
    echo "$binary_path"
}

install_system() {
    echo "Installing garfield-portal (system-wide)..."

    local portal_binary garfield_binary
    portal_binary=$(check_binary "$BINARY_NAME")
    garfield_binary=$(check_binary "$GARFIELD_BINARY")

    # Install portal daemon binary
    sudo install -Dm755 "$portal_binary" "$SYS_BIN_DIR/$BINARY_NAME"
    print_status "Installed binary to $SYS_BIN_DIR/$BINARY_NAME"

    # Install main garfield binary (required for picker mode)
    sudo install -Dm755 "$garfield_binary" "$SYS_BIN_DIR/$GARFIELD_BINARY"
    print_status "Installed binary to $SYS_BIN_DIR/$GARFIELD_BINARY"

    # Install portal file
    sudo install -Dm644 "$SCRIPT_DIR/data/$PORTAL_FILE" "$SYS_PORTAL_DIR/$PORTAL_FILE"
    print_status "Installed portal config to $SYS_PORTAL_DIR/$PORTAL_FILE"

    # Install systemd service (user service, but to system location)
    sudo install -Dm644 "$SCRIPT_DIR/data/$SERVICE_FILE" "/usr/lib/systemd/user/$SERVICE_FILE"
    print_status "Installed systemd service to /usr/lib/systemd/user/$SERVICE_FILE"

    # Install gar-portals.conf
    sudo install -Dm644 "$SCRIPT_DIR/data/gar-portals.conf" "$SYS_PORTALS_CONF_DIR/gar-portals.conf"
    print_status "Installed portal preferences to $SYS_PORTALS_CONF_DIR/gar-portals.conf"

    echo ""
    echo "Installation complete!"
    echo ""
    echo "To enable the portal service, run:"
    echo "  systemctl --user daemon-reload"
    echo "  systemctl --user enable --now garfield-portal"
    echo ""
    echo "To test, run a GTK app with:"
    echo "  GTK_USE_PORTAL=1 gedit"
}

install_user() {
    echo "Installing garfield-portal (user only)..."

    local portal_binary garfield_binary
    portal_binary=$(check_binary "$BINARY_NAME")
    garfield_binary=$(check_binary "$GARFIELD_BINARY")

    # Create directories
    mkdir -p "$USER_BIN_DIR"
    mkdir -p "$USER_PORTAL_DIR"
    mkdir -p "$USER_SERVICE_DIR"
    mkdir -p "$USER_PORTALS_CONF_DIR"

    # Install portal daemon binary
    install -m755 "$portal_binary" "$USER_BIN_DIR/$BINARY_NAME"
    print_status "Installed binary to $USER_BIN_DIR/$BINARY_NAME"

    # Install main garfield binary (required for picker mode)
    install -m755 "$garfield_binary" "$USER_BIN_DIR/$GARFIELD_BINARY"
    print_status "Installed binary to $USER_BIN_DIR/$GARFIELD_BINARY"

    # Install portal file
    install -m644 "$SCRIPT_DIR/data/$PORTAL_FILE" "$USER_PORTAL_DIR/$PORTAL_FILE"
    print_status "Installed portal config to $USER_PORTAL_DIR/$PORTAL_FILE"

    # Install systemd service with modified path
    sed "s|/usr/local/bin/garfield-portal|$USER_BIN_DIR/garfield-portal|g" \
        "$SCRIPT_DIR/data/$SERVICE_FILE" > "$USER_SERVICE_DIR/$SERVICE_FILE"
    print_status "Installed systemd service to $USER_SERVICE_DIR/$SERVICE_FILE"

    # Install gar-portals.conf
    install -m644 "$SCRIPT_DIR/data/gar-portals.conf" "$USER_PORTALS_CONF_DIR/gar-portals.conf"
    print_status "Installed portal preferences to $USER_PORTALS_CONF_DIR/gar-portals.conf"

    echo ""
    echo "Installation complete!"
    echo ""
    echo "Make sure $USER_BIN_DIR is in your PATH."
    echo ""
    echo "To enable the portal service, run:"
    echo "  systemctl --user daemon-reload"
    echo "  systemctl --user enable --now garfield-portal"
    echo ""
    echo "To test, run a GTK app with:"
    echo "  GTK_USE_PORTAL=1 gedit"
}

uninstall_system() {
    echo "Uninstalling garfield-portal (system-wide)..."

    # Stop service first
    systemctl --user stop garfield-portal 2>/dev/null || true
    systemctl --user disable garfield-portal 2>/dev/null || true

    sudo rm -f "$SYS_BIN_DIR/$BINARY_NAME"
    sudo rm -f "$SYS_BIN_DIR/$GARFIELD_BINARY"
    sudo rm -f "/usr/local/sbin/$GARFIELD_BINARY"  # Clean up stray copy
    sudo rm -f "$SYS_PORTAL_DIR/$PORTAL_FILE"
    sudo rm -f "/usr/lib/systemd/user/$SERVICE_FILE"
    sudo rm -f "$SYS_PORTALS_CONF_DIR/gar-portals.conf"

    systemctl --user daemon-reload

    print_status "Uninstalled garfield-portal"
}

uninstall_user() {
    echo "Uninstalling garfield-portal (user only)..."

    # Stop service first
    systemctl --user stop garfield-portal 2>/dev/null || true
    systemctl --user disable garfield-portal 2>/dev/null || true

    rm -f "$USER_BIN_DIR/$BINARY_NAME"
    rm -f "$USER_BIN_DIR/$GARFIELD_BINARY"
    rm -f "$USER_PORTAL_DIR/$PORTAL_FILE"
    rm -f "$USER_SERVICE_DIR/$SERVICE_FILE"
    rm -f "$USER_PORTALS_CONF_DIR/gar-portals.conf"

    systemctl --user daemon-reload

    print_status "Uninstalled garfield-portal"
}

check_installation() {
    echo "Checking garfield-portal installation..."
    echo ""

    # Check portal daemon binary
    if command -v garfield-portal &> /dev/null; then
        print_status "Portal binary found: $(command -v garfield-portal)"
    elif [[ -f "$USER_BIN_DIR/$BINARY_NAME" ]]; then
        print_warning "Portal binary found at $USER_BIN_DIR/$BINARY_NAME (not in PATH)"
    elif [[ -f "$SYS_BIN_DIR/$BINARY_NAME" ]]; then
        print_status "Portal binary found at $SYS_BIN_DIR/$BINARY_NAME"
    else
        print_error "Portal binary not found"
    fi

    # Check main garfield binary (required for picker mode)
    local garfield_path
    if [[ -f "$SYS_BIN_DIR/$GARFIELD_BINARY" ]]; then
        garfield_path="$SYS_BIN_DIR/$GARFIELD_BINARY"
    elif [[ -f "$USER_BIN_DIR/$GARFIELD_BINARY" ]]; then
        garfield_path="$USER_BIN_DIR/$GARFIELD_BINARY"
    elif command -v garfield &> /dev/null; then
        garfield_path="$(command -v garfield)"
    fi

    if [[ -n "$garfield_path" ]]; then
        # Check if garfield has picker mode support
        if "$garfield_path" --help 2>&1 | grep -q "\-\-picker"; then
            print_status "Garfield binary found with picker support: $garfield_path"
        else
            print_error "Garfield binary at $garfield_path does NOT have picker mode!"
            print_error "The portal will not work. Please reinstall."
        fi
    else
        print_error "Garfield binary not found (required for picker mode)"
    fi

    # Check portal config
    if [[ -f "$SYS_PORTAL_DIR/$PORTAL_FILE" ]]; then
        print_status "Portal config found: $SYS_PORTAL_DIR/$PORTAL_FILE"
    elif [[ -f "$USER_PORTAL_DIR/$PORTAL_FILE" ]]; then
        print_status "Portal config found: $USER_PORTAL_DIR/$PORTAL_FILE"
    else
        print_error "Portal config not found"
    fi

    # Check systemd service
    if systemctl --user is-enabled garfield-portal &> /dev/null; then
        print_status "Systemd service enabled"
        if systemctl --user is-active garfield-portal &> /dev/null; then
            print_status "Systemd service running"
        else
            print_warning "Systemd service not running"
        fi
    else
        print_warning "Systemd service not enabled"
    fi

    # Check D-Bus name
    if dbus-send --session --print-reply --dest=org.freedesktop.DBus \
        /org/freedesktop/DBus org.freedesktop.DBus.NameHasOwner \
        string:"org.freedesktop.impl.portal.desktop.garfield" 2>/dev/null | grep -q "true"; then
        print_status "D-Bus service registered"
    else
        print_warning "D-Bus service not registered (portal may not be running)"
    fi
}

show_help() {
    echo "Usage: $0 [OPTION]"
    echo ""
    echo "Install garfield-portal as the system file picker."
    echo ""
    echo "Options:"
    echo "  --user       Install to user directories only (no sudo required)"
    echo "  --uninstall  Remove system installation"
    echo "  --uninstall-user  Remove user installation"
    echo "  --check      Check installation status"
    echo "  --help       Show this help message"
    echo ""
    echo "Without options, installs system-wide (requires sudo)."
}

case "${1:-}" in
    --user)
        install_user
        ;;
    --uninstall)
        uninstall_system
        ;;
    --uninstall-user)
        uninstall_user
        ;;
    --check)
        check_installation
        ;;
    --help|-h)
        show_help
        ;;
    "")
        install_system
        ;;
    *)
        print_error "Unknown option: $1"
        show_help
        exit 1
        ;;
esac
