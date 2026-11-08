#!/usr/bin/env bash
set -euo pipefail

REPO="AchalKumarSharma/speechyyy"
RELEASE_BASE="https://github.com/${REPO}/releases/latest/download"
TARGET_BIN_DIR="${HOME}/.local/bin"
CONFIG_DIR="${SPEECHYYY_CONFIG_DIR:-${HOME}/.config/speechyyy}"
CONFIG_FILE="${SPEECHYYY_CONFIG_FILE:-${CONFIG_DIR}/config.toml}"
HYPR_CONF="${HOME}/.config/hypr/hyprland.conf"
SYSTEMD_USER_DIR="${HOME}/.config/systemd/user"

DRY_RUN="${SPEECHYYY_DRY_RUN:-0}"

# Single runner for side-effecting operations
run() {
    if [ "$DRY_RUN" = "1" ]; then
        echo "[dry-run] Would run: $*"
    else
        "$@"
    fi
}

echo "✦ Initializing Speechyyy installer..."

# 1. Environment & Architecture Detection
ARCH="$(uname -m)"
case "$ARCH" in
    x86_64|amd64)
        TARGET_ARCH="x86_64"
        ;;
    aarch64|arm64)
        TARGET_ARCH="aarch64"
        ;;
    *)
        echo "Error: Unsupported architecture '${ARCH}'. Speechyyy only supports x86_64 and aarch64." >&2
        exit 1
        ;;
esac
echo "[✓] Architecture: ${TARGET_ARCH}"

# Audio subsystem check
if command -v pipewire >/dev/null 2>&1 || pgrep -x pipewire >/dev/null 2>&1 || [ -e "${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/pipewire-0" ]; then
    echo "[✓] PipeWire audio subsystem detected."
else
    echo "[!] Warning: PipeWire not detected. Speechyyy requires PipeWire/ALSA for low-latency microphone capture."
fi

# Wayland display check
if [ -n "${WAYLAND_DISPLAY:-}" ] || [ "${XDG_SESSION_TYPE:-}" = "wayland" ] || [[ "${XDG_CURRENT_DESKTOP:-}" =~ [Hh]yprland|[Ss]way ]]; then
    echo "[✓] Wayland compositor session detected."
else
    echo "[!] Warning: Wayland session not detected (\$WAYLAND_DISPLAY / \$XDG_CURRENT_DESKTOP unset). speechyyy-dock requires a Wayland compositor supporting wlr-layer-shell."
fi

# Clipboard tool check (wl-clipboard / wl-copy)
if command -v wl-copy >/dev/null 2>&1; then
    echo "[✓] wl-clipboard (wl-copy) detected for instant paste."
else
    echo "[!] Warning: wl-copy not found. Speechyyy defaults to clipboard paste via 'wl-clipboard'."
    echo "    Please install the 'wl-clipboard' package (e.g. 'sudo pacman -S wl-clipboard' or 'sudo apt install wl-clipboard'), or set injection_mode = \"typewriter\" in config.toml."
fi

# 2. Binary Installation
IS_LOCAL_CHECKOUT=0
if [ -f "Cargo.toml" ] && grep -q 'name = "speechyyy"' Cargo.toml 2>/dev/null; then
    if [ -f "target/release/speechyyy" ] && [ -f "target/release/speechyyy-dock" ]; then
        IS_LOCAL_CHECKOUT=1
    fi
fi

if [ "$IS_LOCAL_CHECKOUT" = "1" ]; then
    echo "==> Local release artifacts detected."
    if [ "$DRY_RUN" = "1" ]; then
        echo "[dry-run] Would create directory ${TARGET_BIN_DIR}"
        echo "[dry-run] Would install target/release/speechyyy to ${TARGET_BIN_DIR}/speechyyy (mode 0755)"
        echo "[dry-run] Would install target/release/speechyyy-dock to ${TARGET_BIN_DIR}/speechyyy-dock (mode 0755)"
    else
        mkdir -p "$TARGET_BIN_DIR"
        install -m 755 "target/release/speechyyy" "${TARGET_BIN_DIR}/speechyyy"
        install -m 755 "target/release/speechyyy-dock" "${TARGET_BIN_DIR}/speechyyy-dock"
        echo "[✓] Installed release binaries to ${TARGET_BIN_DIR}/"
    fi
else
    if [ "$DRY_RUN" = "1" ]; then
        echo "[dry-run] Would create directory ${TARGET_BIN_DIR}"
        echo "[dry-run] Would download release binaries and SHA256SUMS from ${RELEASE_BASE} for ${TARGET_ARCH}"
        echo "[dry-run] Would verify SHA256 checksums before installation"
        echo "[dry-run] Would install speechyyy and speechyyy-dock to ${TARGET_BIN_DIR}/ (mode 0755)"
    else
        TMP_DIR=$(mktemp -d)
        cleanup() {
            rm -rf "$TMP_DIR"
        }
        trap cleanup EXIT

        echo "==> Downloading release assets from ${RELEASE_BASE}..."
        DOWNLOAD_SUCCESS=0
        if curl -fsSL "${RELEASE_BASE}/SHA256SUMS" -o "${TMP_DIR}/SHA256SUMS" 2>/dev/null; then
            TARBALL_NAME="speechyyy-${TARGET_ARCH}-unknown-linux-gnu.tar.gz"
            if curl -fsSL "${RELEASE_BASE}/${TARBALL_NAME}" -o "${TMP_DIR}/${TARBALL_NAME}" 2>/dev/null; then
                echo "==> Verifying SHA256 checksum..."
                (
                    cd "$TMP_DIR"
                    if grep -F "$TARBALL_NAME" SHA256SUMS | sha256sum -c --status 2>/dev/null; then
                        exit 0
                    else
                        exit 1
                    fi
                )
                if [ $? -eq 0 ]; then
                    echo "[✓] SHA256 checksum verified."
                    tar -xzf "${TMP_DIR}/${TARBALL_NAME}" -C "${TMP_DIR}"
                    mkdir -p "$TARGET_BIN_DIR"
                    if [ -f "${TMP_DIR}/speechyyy" ] && [ -f "${TMP_DIR}/speechyyy-dock" ]; then
                        install -m 755 "${TMP_DIR}/speechyyy" "${TARGET_BIN_DIR}/speechyyy"
                        install -m 755 "${TMP_DIR}/speechyyy-dock" "${TARGET_BIN_DIR}/speechyyy-dock"
                        DOWNLOAD_SUCCESS=1
                    elif [ -f "${TMP_DIR}/target/release/speechyyy" ] && [ -f "${TMP_DIR}/target/release/speechyyy-dock" ]; then
                        install -m 755 "${TMP_DIR}/target/release/speechyyy" "${TARGET_BIN_DIR}/speechyyy"
                        install -m 755 "${TMP_DIR}/target/release/speechyyy-dock" "${TARGET_BIN_DIR}/speechyyy-dock"
                        DOWNLOAD_SUCCESS=1
                    fi
                else
                    echo "Error: SHA256 checksum verification failed for ${TARBALL_NAME}!" >&2
                    cleanup
                    exit 1
                fi
            fi
        fi

        if [ "$DOWNLOAD_SUCCESS" = "0" ]; then
            echo "[!] Pre-built binary download unavailable. Checking for cargo to build from source..."
            if command -v cargo >/dev/null 2>&1 && [ -f "Cargo.toml" ] && grep -q 'name = "speechyyy"' Cargo.toml 2>/dev/null; then
                echo "==> Compiling from source via cargo build --release..."
                cargo build --release --bin speechyyy --bin speechyyy-dock
                mkdir -p "$TARGET_BIN_DIR"
                install -m 755 "target/release/speechyyy" "${TARGET_BIN_DIR}/speechyyy"
                install -m 755 "target/release/speechyyy-dock" "${TARGET_BIN_DIR}/speechyyy-dock"
            else
                echo "Error: Failed to download release binaries from GitHub and cargo is not available to build from source." >&2
                echo "Please install cargo (https://rustup.rs/) or check release assets at: https://github.com/${REPO}/releases" >&2
                exit 1
            fi
        fi
    fi
fi

# PATH validation
case ":${PATH}:" in
    *:"${TARGET_BIN_DIR}":*) ;;
    *)
        echo ""
        echo "[!] Notice: ${TARGET_BIN_DIR} is not in your current PATH."
        echo "    Add it to your shell configuration (~/.bashrc, ~/.zshrc, or ~/.config/fish/config.fish):"
        echo "      export PATH=\"\$HOME/.local/bin:\$PATH\""
        echo ""
        ;;
esac

# 3. Udev Rule for /dev/uinput (Non-Root Input)
echo "✦ Checking /dev/uinput write permissions for typing injection..."
UDEV_RULE_FILE="/etc/udev/rules.d/99-uinput.rules"
UDEV_RULE_CONTENT='KERNEL=="uinput", SUBSYSTEM=="misc", OPTIONS+="static_node=uinput", TAG+="uaccess"'
UINPUT_CONFIGURED=0

if [ -w /dev/uinput ]; then
    echo "[✓] /dev/uinput is writable by current user."
    UINPUT_CONFIGURED=1
elif [ -f "$UDEV_RULE_FILE" ] && grep -qxF "$UDEV_RULE_CONTENT" "$UDEV_RULE_FILE" 2>/dev/null; then
    echo "[✓] Udev rule ${UDEV_RULE_FILE} already installed with matching content."
    echo "✦ Note: /dev/uinput is not yet writable in this session. You may need to log out and back in (uaccess ACLs apply per session)."
    UINPUT_CONFIGURED=2
else
    echo "✦ Typing injection requires non-root uaccess permissions for /dev/uinput."
    PROCEED_UDEV=1
    if [ "$DRY_RUN" = "0" ]; then
        if [ -t 0 ]; then
            read -rp "✦ Configure /etc/udev/rules.d/99-uinput.rules via sudo? [Y/n] " UDEV_REPLY || UDEV_REPLY=""
            case "$UDEV_REPLY" in
                [nN]*) PROCEED_UDEV=0 ;;
                *) PROCEED_UDEV=1 ;;
            esac
        elif [ ! "/dev/null" -ef /dev/stdin ] && [ -r /dev/tty ] && (exec < /dev/tty 2>/dev/null); then
            read -rp "✦ Configure /etc/udev/rules.d/99-uinput.rules via sudo? [Y/n] " UDEV_REPLY < /dev/tty || UDEV_REPLY=""
            case "$UDEV_REPLY" in
                [nN]*) PROCEED_UDEV=0 ;;
                *) PROCEED_UDEV=1 ;;
            esac
        fi
    fi

    if [ "$PROCEED_UDEV" = "1" ]; then
        if [ "$DRY_RUN" = "1" ]; then
            echo "[dry-run] Would run: sudo modprobe uinput"
            echo "[dry-run] Would run: echo 'uinput' | sudo tee /etc/modules-load.d/uinput.conf > /dev/null"
            echo "[dry-run] Would run: echo '${UDEV_RULE_CONTENT}' | sudo tee ${UDEV_RULE_FILE} > /dev/null"
            echo "[dry-run] Would run: sudo udevadm control --reload-rules"
            echo "[dry-run] Would run: sudo udevadm trigger /dev/uinput"
        else
            sudo modprobe uinput || true
            echo 'uinput' | sudo tee /etc/modules-load.d/uinput.conf > /dev/null
            echo "$UDEV_RULE_CONTENT" | sudo tee "$UDEV_RULE_FILE" > /dev/null
            sudo udevadm control --reload-rules
            sudo udevadm trigger /dev/uinput

            if [ -w /dev/uinput ]; then
                echo "[✓] Hardware typing permissions (/dev/uinput) configured successfully."
                UINPUT_CONFIGURED=1
            else
                echo "✦ Note: Udev rule installed, but /dev/uinput is not yet writable in this session."
                echo "  Please log out and log back in for uaccess ACLs to take effect."
                UINPUT_CONFIGURED=2
            fi
        fi
    else
        echo "[!] Skipped udev configuration. Note that keystroke injection may fail without uinput permissions."
    fi
fi

# 4. Secure Configuration Setup (~/.config/speechyyy/config.toml)
echo "✦ Checking configuration at ${CONFIG_FILE}..."
API_KEY_STATUS="already_configured"
if [ -f "$CONFIG_FILE" ]; then
    echo "[✓] Configuration file exists. Preserving ${CONFIG_FILE} untouched."
else
    API_KEY=""
    if [ -n "${SPEECHYYY_API_KEY:-}" ]; then
        API_KEY="$SPEECHYYY_API_KEY"
    elif [ -t 0 ]; then
        read -rsp "✦ Enter your Sarvam AI API Key (press Enter to skip): " API_KEY || API_KEY=""
        echo ""
    elif [ ! "/dev/null" -ef /dev/stdin ] && [ -r /dev/tty ] && (exec < /dev/tty 2>/dev/null); then
        read -rsp "✦ Enter your Sarvam AI API Key (press Enter to skip): " API_KEY < /dev/tty || API_KEY=""
        echo ""
    else
        API_KEY=""
    fi

    # Escape API key for TOML string (escape \ and ")
    ESCAPED_KEY="${API_KEY//\\/\\\\}"
    ESCAPED_KEY="${ESCAPED_KEY//\"/\\\"}"

    if [ "$DRY_RUN" = "1" ]; then
        echo "[dry-run] Would create directory ${CONFIG_DIR}"
        echo "[dry-run] Would write secure configuration to ${CONFIG_FILE} (mode 0600)"
        if [ -z "$API_KEY" ]; then
            echo "[dry-run] ✦ No API key provided. Add one later in ${CONFIG_FILE}"
            API_KEY_STATUS="skipped"
        else
            echo "[dry-run] [✓] API key configured securely."
            API_KEY_STATUS="configured"
        fi
    else
        mkdir -p "$CONFIG_DIR"
        (
            umask 077
            cat > "$CONFIG_FILE" <<EOF
hotkey = "RightAlt"
pacing_delay_ms = 15
# Injection mode:
# - "clipboard" (default): fast paste via Ctrl+V; leaves transcription in clipboard and immune to browser hotkey hijacking. (Requires wl-clipboard on Wayland; does not paste in standard terminals).
# - "typewriter": character-by-character typing via /dev/uinput. Works in terminals, but triggers page shortcuts if no text field is focused.
injection_mode = "clipboard"

[input]
hotkey = "RightAlt"
grab_key = true

[audio]
sample_rate = 16000
vad_threshold = 0.005
min_duration_ms = 200

[inference]
provider = "sarvam"
api_key = "${ESCAPED_KEY}"
model = "saaras:v4"
language_code = "unknown"
mode = "translit"

[injector]
method = "clipboard"
pacing_delay_ms = 15

[ui]
nickname = "babe"
EOF
        )
        chmod 600 "$CONFIG_FILE"

        if [ -z "$API_KEY" ]; then
            echo "✦ No API key provided. Add one later in ${CONFIG_FILE}"
            API_KEY_STATUS="skipped"
        else
            echo "[✓] Configuration written securely with mode 0600."
            API_KEY_STATUS="configured"
        fi
    fi
fi

# 5. Autostart Integration
echo "✦ Checking autostart configuration..."
HYPR_STATUS="not_detected"
if [ -f "$HYPR_CONF" ]; then
    HYPR_LINES=(
        "exec-once = speechyyy daemon"
        "exec-once = speechyyy-dock"
        "bind = , code:108, exec, speechyyy toggle"
    )

    MISSING_LINES=()
    for line in "${HYPR_LINES[@]}"; do
        if ! grep -qxF "$line" "$HYPR_CONF" 2>/dev/null; then
            MISSING_LINES+=("$line")
        fi
    done

    if [ "${#MISSING_LINES[@]}" -eq 0 ]; then
        echo "[✓] Hyprland configuration (${HYPR_CONF}) already contains Speechyyy autostart & hotkey."
        HYPR_STATUS="already_present"
    else
        PROCEED_HYPR=1
        if [ "$DRY_RUN" = "0" ]; then
            if [ -t 0 ]; then
                read -rp "✦ Add Speechyyy autostart & hotkey (RightAlt) to ~/.config/hypr/hyprland.conf? [Y/n] " HYPR_REPLY || HYPR_REPLY=""
                case "$HYPR_REPLY" in
                    [nN]*) PROCEED_HYPR=0 ;;
                    *) PROCEED_HYPR=1 ;;
                esac
            elif [ ! "/dev/null" -ef /dev/stdin ] && [ -r /dev/tty ] && (exec < /dev/tty 2>/dev/null); then
                read -rp "✦ Add Speechyyy autostart & hotkey (RightAlt) to ~/.config/hypr/hyprland.conf? [Y/n] " HYPR_REPLY < /dev/tty || HYPR_REPLY=""
                case "$HYPR_REPLY" in
                    [nN]*) PROCEED_HYPR=0 ;;
                    *) PROCEED_HYPR=1 ;;
                esac
            fi
        fi

        if [ "$PROCEED_HYPR" = "1" ]; then
            if [ "$DRY_RUN" = "1" ]; then
                echo "[dry-run] Would backup ${HYPR_CONF} to ${HYPR_CONF}.bak.<timestamp>"
                for line in "${MISSING_LINES[@]}"; do
                    echo "[dry-run] Would append to ${HYPR_CONF}: ${line}"
                done
                HYPR_STATUS="would_update"
            else
                BACKUP_PATH="${HYPR_CONF}.bak.$(date +%s)"
                cp "$HYPR_CONF" "$BACKUP_PATH"
                for line in "${MISSING_LINES[@]}"; do
                    echo "$line" >> "$HYPR_CONF"
                done
                echo "[✓] Hyprland configuration updated (backup created at ${BACKUP_PATH})."
                HYPR_STATUS="updated"
            fi
        else
            echo "[!] Skipped Hyprland configuration update."
            HYPR_STATUS="skipped"
        fi
    fi
fi

# Systemd user service generation
if [ "$DRY_RUN" = "1" ]; then
    echo "[dry-run] Would write systemd user services to ${SYSTEMD_USER_DIR}/ (speechyyy.service, speechyyy-dock.service)"
else
    mkdir -p "$SYSTEMD_USER_DIR"
    cat > "${SYSTEMD_USER_DIR}/speechyyy.service" <<EOF
[Unit]
Description=Speechyyy Ambient Dictation Daemon
Documentation=https://github.com/${REPO}
After=pipewire.service sound.target

[Service]
Type=simple
ExecStart=%h/.local/bin/speechyyy daemon --foreground
Restart=on-failure
RestartSec=2s

[Install]
WantedBy=default.target
EOF

    cat > "${SYSTEMD_USER_DIR}/speechyyy-dock.service" <<EOF
[Unit]
Description=Speechyyy Wayland Companion Dock
Documentation=https://github.com/${REPO}
PartOf=graphical-session.target
After=graphical-session.target speechyyy.service

[Service]
Type=simple
ExecStart=%h/.local/bin/speechyyy-dock
Restart=on-failure
RestartSec=2s

[Install]
WantedBy=graphical-session.target
EOF
    echo "[✓] Generated systemd user service templates in ${SYSTEMD_USER_DIR}/"
fi

# 6. Final Summary
echo ""
echo "✦ Speechyyy v0.2.0 Installation Summary"
echo "────────────────────────────────────────────"
echo "• Binaries:       ${TARGET_BIN_DIR}/speechyyy, ${TARGET_BIN_DIR}/speechyyy-dock (mode 0755)"
if [ -f "$CONFIG_FILE" ] && [ "$API_KEY_STATUS" = "already_configured" ]; then
    echo "• Configuration:  ${CONFIG_FILE} (preserved untouched)"
else
    echo "• Configuration:  ${CONFIG_FILE} (mode 0600)"
fi
case "$API_KEY_STATUS" in
    configured)
        echo "• Sarvam API Key: Configured securely in config.toml"
        ;;
    skipped)
        echo "• Sarvam API Key: Skipped (add anytime in ~/.config/speechyyy/config.toml)"
        ;;
    already_configured)
        echo "• Sarvam API Key: Preserved in existing configuration"
        ;;
esac
case "$UINPUT_CONFIGURED" in
    1)
        echo "• Hardware Input: /dev/uinput writable"
        ;;
    2)
        echo "• Hardware Input: Udev rule installed (please re-login for ACLs to take effect)"
        ;;
    *)
        echo "• Hardware Input: Udev rule not configured"
        ;;
esac
case "$HYPR_STATUS" in
    updated)
        echo "• Autostart:      Hyprland config updated (~/.config/hypr/hyprland.conf)"
        ;;
    already_present)
        echo "• Autostart:      Hyprland config already active"
        ;;
    would_update)
        echo "• Autostart:      Hyprland config planned for update (dry-run)"
        ;;
    skipped)
        echo "• Autostart:      Hyprland update skipped by user"
        ;;
    not_detected)
        echo "• Autostart:      Systemd service templates available in ~/.config/systemd/user/"
        ;;
esac

echo ""
echo "Optional Systemd Autostart:"
echo "  To enable and run Speechyyy via systemd instead of Hyprland exec-once:"
echo "    systemctl --user daemon-reload"
echo "    systemctl --user enable --now speechyyy.service speechyyy-dock.service"
echo "  (Note: Do not enable both Hyprland exec-once and systemd to prevent duplicate instances)"
echo ""
echo "Next Steps:"
echo "  1. Test your setup:"
echo "     speechyyy doctor"
echo "  2. Start Speechyyy manually (if not autostarted):"
echo "     speechyyy daemon & speechyyy-dock &"
echo "  3. Dictate anywhere:"
echo "     Hold [RightAlt] to speak, release to type directly at your cursor!"
echo ""
