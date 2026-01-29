#!/bin/bash
# Test script for garfield-portal
#
# This script verifies that the portal is working correctly.

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

print_status() { echo -e "${GREEN}[PASS]${NC} $1"; }
print_warning() { echo -e "${YELLOW}[WARN]${NC} $1"; }
print_error() { echo -e "${RED}[FAIL]${NC} $1"; }

echo "Testing garfield-portal..."
echo ""

# Test 1: Check if D-Bus name is registered
echo "1. Checking D-Bus registration..."
if busctl --user status org.freedesktop.impl.portal.desktop.garfield &>/dev/null; then
    print_status "D-Bus name registered"
else
    print_error "D-Bus name not registered"
    echo "   Try: systemctl --user start garfield-portal"
    exit 1
fi

# Test 2: Check if FileChooser interface is available
echo ""
echo "2. Checking FileChooser interface..."
if busctl --user introspect org.freedesktop.impl.portal.desktop.garfield /org/freedesktop/portal/desktop 2>/dev/null | grep -q "OpenFile"; then
    print_status "FileChooser.OpenFile method available"
else
    print_error "FileChooser interface not found"
    exit 1
fi

# Test 3: Check portal config
echo ""
echo "3. Checking portal configuration..."
if [[ -f /usr/share/xdg-desktop-portal/portals/garfield.portal ]] || \
   [[ -f ~/.local/share/xdg-desktop-portal/portals/garfield.portal ]]; then
    print_status "Portal config file exists"
else
    print_error "Portal config not found"
    exit 1
fi

# Test 4: Check if garfield binary is available
echo ""
echo "4. Checking garfield binary..."
if command -v garfield &>/dev/null; then
    print_status "garfield binary found: $(command -v garfield)"
else
    print_warning "garfield not in PATH (needed for picker mode)"
fi

echo ""
echo "All tests passed!"
echo ""
echo "To test with a real application:"
echo ""
echo "  # With GTK apps:"
echo "  GTK_USE_PORTAL=1 gedit"
echo "  GTK_USE_PORTAL=1 firefox"
echo ""
echo "  # With Qt apps (KDE):"
echo "  QT_QPA_PLATFORMTHEME=xdgdesktopportal dolphin"
echo ""
echo "  # With Flatpak apps:"
echo "  flatpak run org.gnome.TextEditor"
echo ""
echo "To use garfield as the default file picker in gar desktop,"
echo "set XDG_CURRENT_DESKTOP=gar in your session."
