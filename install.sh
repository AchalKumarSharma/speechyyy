#!/usr/bin/env bash
set -e

# Check if script is run in repository root containing Cargo.toml
if [ ! -f "Cargo.toml" ]; then
    echo "Error: install.sh must be run from the repository root containing Cargo.toml." >&2
    exit 1
fi

echo "==> Building Speechyyy in release mode..."
cargo build --release

# Copy binary to ~/.local/bin/speechyyy
INSTALL_DIR="${HOME}/.local/bin"
mkdir -p "${INSTALL_DIR}"
cp "target/release/speechyyy" "${INSTALL_DIR}/speechyyy"
chmod +x "${INSTALL_DIR}/speechyyy"
echo "[✓] Installed speechyyy binary to ${INSTALL_DIR}/speechyyy"

# Ensure uinput kernel module is active
echo "==> Checking uinput kernel module..."
if [ ! -e "/dev/uinput" ]; then
    echo "Notice: /dev/uinput does not exist. Loading module with sudo modprobe uinput..."
    sudo modprobe uinput
else
    echo "[✓] /dev/uinput is active."
fi

# Offer to persist uinput via /etc/modules-load.d/uinput.conf if not present
if [ ! -f "/etc/modules-load.d/uinput.conf" ]; then
    echo "Notice: /etc/modules-load.d/uinput.conf does not exist."
    SHOULD_PERSIST="n"
    if [ -t 0 ]; then
        read -r -p "Persist uinput module across reboots (/etc/modules-load.d/uinput.conf)? [Y/n] " answer
        case "${answer:-Y}" in
            [yY][eE][sS]|[yY])
                SHOULD_PERSIST="y"
                ;;
            *)
                SHOULD_PERSIST="n"
                ;;
        esac
    else
        SHOULD_PERSIST="y"
    fi

    if [ "$SHOULD_PERSIST" = "y" ]; then
        sudo mkdir -p /etc/modules-load.d
        echo "uinput" | sudo tee /etc/modules-load.d/uinput.conf > /dev/null
        echo "[✓] Persisted uinput in /etc/modules-load.d/uinput.conf"
    fi
else
    echo "[✓] uinput module persistence already configured in /etc/modules-load.d/uinput.conf"
fi

# Install udev rule
echo "==> Installing udev rule..."
if [ -f "extra/99-speechyyy-uinput.rules" ]; then
    sudo cp "extra/99-speechyyy-uinput.rules" "/etc/udev/rules.d/99-speechyyy-uinput.rules"
    sudo udevadm control --reload-rules && sudo udevadm trigger
    echo "[✓] Udev rule installed and reloaded."
else
    echo "Error: extra/99-speechyyy-uinput.rules not found!" >&2
    exit 1
fi

# Run doctor diagnostic
echo "==> Running system diagnostics (speechyyy doctor)..."
"${INSTALL_DIR}/speechyyy" doctor || true

echo ""
echo "[✓] Speechyyy installation complete!"
case ":$PATH:" in
    *:"$INSTALL_DIR":*) ;;
    *)
        echo "[!] Notice: ${INSTALL_DIR} is not in your current PATH."
        echo "    Add it by running: export PATH=\"\$HOME/.local/bin:\$PATH\""
        echo "    or append it to your ~/.bashrc or ~/.zshrc."
        ;;
esac

