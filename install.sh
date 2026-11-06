#!/usr/bin/env bash
set -e

REPO="achalkumarsharma/speechyyy"
TARGET_DIR="${HOME}/.local/bin"
mkdir -p "${TARGET_DIR}"

UDEV_RULE_SRC=""

if [ -f "Cargo.toml" ] && command -v cargo >/dev/null 2>&1; then
    echo "==> Local repository detected. Compiling release binary via cargo..."
    cargo build --release
    cp "target/release/speechyyy" "${TARGET_DIR}/speechyyy"
    find target/release/ -maxdepth 1 -name "*.so*" -exec cp -d {} "${TARGET_DIR}/" \; 2>/dev/null || true
    chmod +x "${TARGET_DIR}/speechyyy"
    UDEV_RULE_SRC="extra/99-speechyyy-uinput.rules"
else
    echo "==> Fetching latest pre-built release for Linux (x86_64)..."
    ARCH="$(uname -m)"
    if [ "$ARCH" != "x86_64" ]; then
        echo "Error: Unsupported architecture: $ARCH. Pre-built binaries are currently only available for x86_64." >&2
        exit 1
    fi

    LATEST_TAG=$(curl -fsSLI -o /dev/null -w "%{url_effective}" "https://github.com/${REPO}/releases/latest" | tr '/' '\n' | tail -n 1)
    if [ -z "$LATEST_TAG" ] || [ "$LATEST_TAG" = "releases" ] || [[ "$LATEST_TAG" == *"releases"* ]]; then
        echo "Error: Could not determine latest release tag for ${REPO}." >&2
        echo "Please check https://github.com/${REPO}/releases manually." >&2
        exit 1
    fi

    TMP_DIR=$(mktemp -d)
    trap 'rm -rf "${TMP_DIR}"' EXIT

    TARBALL_URL="https://github.com/${REPO}/releases/download/${LATEST_TAG}/speechyyy-${LATEST_TAG}-x86_64-unknown-linux-gnu.tar.gz"
    echo "==> Downloading pre-built release ${LATEST_TAG}..."
    curl -fsSL "$TARBALL_URL" -o "${TMP_DIR}/speechyyy.tar.gz"

    echo "==> Extracting release archive..."
    tar -xzf "${TMP_DIR}/speechyyy.tar.gz" -C "${TMP_DIR}"

    if [ -f "${TMP_DIR}/speechyyy" ]; then
        cp "${TMP_DIR}/speechyyy" "${TARGET_DIR}/speechyyy"
    elif [ -f "${TMP_DIR}/target/release/speechyyy" ]; then
        cp "${TMP_DIR}/target/release/speechyyy" "${TARGET_DIR}/speechyyy"
    else
        BIN_FOUND=$(find "${TMP_DIR}" -type f -name "speechyyy" | head -n 1)
        if [ -n "$BIN_FOUND" ]; then
            cp "$BIN_FOUND" "${TARGET_DIR}/speechyyy"
        else
            echo "Error: Could not locate speechyyy binary in release archive." >&2
            exit 1
        fi
    fi
    find "${TMP_DIR}" -maxdepth 1 -name "*.so*" -exec cp -d {} "${TARGET_DIR}/" \; 2>/dev/null || true
    chmod +x "${TARGET_DIR}/speechyyy"

    if [ -f "${TMP_DIR}/extra/99-speechyyy-uinput.rules" ]; then
        UDEV_RULE_SRC="${TMP_DIR}/extra/99-speechyyy-uinput.rules"
    elif [ -f "${TMP_DIR}/99-speechyyy-uinput.rules" ]; then
        UDEV_RULE_SRC="${TMP_DIR}/99-speechyyy-uinput.rules"
    else
        UDEV_RULE_SRC=$(find "${TMP_DIR}" -type f -name "99-speechyyy-uinput.rules" | head -n 1)
    fi
fi

# Kernel & Udev Configuration
echo "==> Configuring uinput kernel module..."
if [ ! -e "/dev/uinput" ]; then
    echo "Notice: /dev/uinput does not exist. Loading module via sudo modprobe uinput..."
    sudo modprobe uinput || true
else
    echo "[✓] /dev/uinput is active."
fi

TARGET_USER="${SUDO_USER:-$USER}"
echo "==> Configuring instant hardware permissions for user ${TARGET_USER}..."
sudo setfacl -m u:"$TARGET_USER":rw /dev/uinput 2>/dev/null || true

if [ -n "$UDEV_RULE_SRC" ] && [ -f "$UDEV_RULE_SRC" ]; then
    echo "==> Installing udev rule to /etc/udev/rules.d/99-speechyyy-uinput.rules..."
    sudo cp "$UDEV_RULE_SRC" "/etc/udev/rules.d/99-speechyyy-uinput.rules"
    sudo udevadm control --reload-rules && sudo udevadm trigger
    echo "[✓] Udev rule installed and reloaded."
else
    echo "[!] Warning: Udev rule not found; skipping udev setup."
fi

# PATH Notification
case ":$PATH:" in
    *:"${TARGET_DIR}":*) ;;
    *)
        echo ""
        echo "[!] Notice: ${TARGET_DIR} is not in your current PATH."
        echo "    Add it by placing this line in your ~/.bashrc or ~/.zshrc:"
        echo "      export PATH=\"\$HOME/.local/bin:\$PATH\""
        echo ""
        ;;
esac

echo ""
echo "[✓] Speechyyy installed successfully to ${TARGET_DIR}/speechyyy"
echo ""
"${TARGET_DIR}/speechyyy" --help
