#!/bin/bash
# 🛡️ TylluanNexus o3 — Sovereign Linux Installer
# "The AI hub that stays local."

set -e

echo "--------------------------------------------------"
echo "🛡️ TYLLUANNEXUS o3 — Sovereign Installation"
echo "--------------------------------------------------"

# 1. Detect Architecture
ARCH=$(uname -m)
OS=$(uname -s)

if [ "$OS" != "Linux" ]; then
    echo "❌ This installer is for Linux. Please use the appropriate installer for $OS."
    exit 1
fi

case $ARCH in
    x86_64)  TARGET="x86_64-unknown-linux-musl" ;;
    aarch64) TARGET="aarch64-unknown-linux-musl" ;;
    *) echo "❌ Unsupported architecture: $ARCH"; exit 1 ;;
esac

echo "✅ Detected: $ARCH ($TARGET)"

# 2. Setup Directories
INSTALL_DIR="/opt/tylluan-nexus"
BIN_DIR="/usr/local/bin"

sudo mkdir -p "$INSTALL_DIR"
sudo mkdir -p "$INSTALL_DIR/data"
sudo mkdir -p "$INSTALL_DIR/logs"
sudo mkdir -p "$INSTALL_DIR/models"

# 3. Download Binaries (Simulated URL for now - T19 will finalize the S3/GitHub flow)
echo "📥 Provisioning sovereign binaries..."
# sudo curl -L "https://github.com/TylluanMCP/tylluan-nexus/releases/latest/download/tylluan-nexus-$TARGET" -o "$INSTALL_DIR/tylluan-nexus"
# sudo curl -L "https://github.com/TylluanMCP/tylluan-nexus/releases/latest/download/tylluan-cli-$TARGET" -o "$INSTALL_DIR/tylluan"

# For local testing/dev, if the binaries exist in the current build context, we copy them.
if [ -f "./target/$TARGET/release/tylluan-nexus" ]; then
    sudo cp "./target/$TARGET/release/tylluan-nexus" "$INSTALL_DIR/"
    sudo cp "./target/$TARGET/release/tylluan-cli" "$INSTALL_DIR/tylluan"
fi

sudo chmod +x "$INSTALL_DIR/tylluan-nexus"
sudo chmod +x "$INSTALL_DIR/tylluan"

# Link to PATH
sudo ln -sf "$INSTALL_DIR/tylluan" "$BIN_DIR/tylluan"

# 4. Create Service (systemd)
echo "⚙️  Configuring systemd service..."
sudo bash -c "cat <<EOF > /etc/systemd/system/tylluan-nexus.service
[Unit]
Description=TylluanNexus Sovereign Agentic Hub
After=network.target

[Service]
Type=simple
User=$USER
WorkingDirectory=$INSTALL_DIR
ExecStart=$INSTALL_DIR/tylluan-nexus --headless
Restart=always
RestartSec=10
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
EOF"

sudo systemctl daemon-reload
sudo systemctl enable tylluan-nexus

echo "--------------------------------------------------"
echo "✅ Installation Complete!"
echo "--------------------------------------------------"
echo "Usage:"
echo "  tylluan start   - Start the hub"
echo "  tylluan status  - Check health"
echo "  tylluan logs    - See what's happening"
echo "--------------------------------------------------"
