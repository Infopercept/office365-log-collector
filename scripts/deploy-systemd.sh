#!/bin/bash
# Deployment Script for Office365 Audit Log Collector (Systemd-based)
# This script installs the collector and fluentd as native systemd services

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

echo "=========================================="
echo "Office365 Collector - Systemd Deployment"
echo "=========================================="

# Check if running as root or with sudo
if [ "$EUID" -ne 0 ]; then
    echo "Please run with sudo: sudo $0"
    exit 1
fi

# Get the actual user (not root)
ACTUAL_USER=${SUDO_USER:-$USER}
ACTUAL_HOME=$(eval echo ~$ACTUAL_USER)

echo "Installing for user: $ACTUAL_USER"
echo "Project directory: $PROJECT_DIR"
echo ""

# Step 1: Install Fluentd
echo "[1/6] Installing Fluentd..."
if ! command -v fluentd &> /dev/null; then
    curl -fsSL https://toolbelt.treasuredata.com/sh/install-ubuntu-jammy-fluent-package5-lts.sh | sh
    echo "Fluentd installed successfully"
else
    echo "Fluentd already installed"
fi

# Step 2: Build the collector
echo ""
echo "[2/6] Building Office365 Collector..."
cd "$PROJECT_DIR"
if command -v cargo &> /dev/null; then
    sudo -u $ACTUAL_USER cargo build --release
else
    echo "ERROR: Rust/Cargo not installed. Install from https://rustup.rs/"
    exit 1
fi

# Step 3: Create config if not exists
echo ""
echo "[3/6] Setting up configuration..."
if [ ! -f "$PROJECT_DIR/config/config.yaml" ]; then
    if [ -f "$PROJECT_DIR/config/config.yaml.template" ]; then
        cp "$PROJECT_DIR/config/config.yaml.template" "$PROJECT_DIR/config/config.yaml"
        chown $ACTUAL_USER:$ACTUAL_USER "$PROJECT_DIR/config/config.yaml"
        chmod 600 "$PROJECT_DIR/config/config.yaml"
        echo "Created config.yaml from template"
        echo "WARNING: Please edit $PROJECT_DIR/config/config.yaml with your Office365 credentials!"
    fi
else
    echo "config.yaml already exists"
fi

# Step 4: Setup Fluentd config
echo ""
echo "[4/6] Configuring Fluentd..."
mkdir -p /var/log/fluent/office365
chown -R _fluentd:_fluentd /var/log/fluent/office365
cp "$PROJECT_DIR/fluentd/fluentd-systemd.conf" /etc/fluent/fluentd.conf
echo "Fluentd configuration installed"

# Step 5: Install systemd service for collector
echo ""
echo "[5/6] Installing systemd services..."

# Update service file with correct paths
cat > /etc/systemd/system/office365-collector.service << EOF
[Unit]
Description=Office365 Audit Log Collector
After=network.target fluentd.service

[Service]
Type=simple
User=$ACTUAL_USER
WorkingDirectory=$PROJECT_DIR
ExecStart=$PROJECT_DIR/target/release/office_audit_log_collector --config $PROJECT_DIR/config/config.yaml
Restart=always
RestartSec=10
LimitNOFILE=65535

# Logging
StandardOutput=journal
StandardError=journal
SyslogIdentifier=office365-collector

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
echo "Systemd service installed"

# Step 6: Enable and start services
echo ""
echo "[6/6] Starting services..."
systemctl enable fluentd
systemctl enable office365-collector
systemctl restart fluentd
sleep 2
systemctl restart office365-collector

echo ""
echo "=========================================="
echo "Deployment Complete!"
echo "=========================================="
echo ""
echo "Services Status:"
systemctl status fluentd --no-pager | head -5
echo ""
systemctl status office365-collector --no-pager | head -5
echo ""
echo "Output files: /var/log/fluent/office365/"
echo "  - AuditAzureActiveDirectory.json"
echo "  - AuditExchange.json"
echo "  - AuditSharePoint.json"
echo "  - AuditGeneral.json"
echo "  - DLPAll.json"
echo ""
echo "Commands:"
echo "  View collector logs:  journalctl -u office365-collector -f"
echo "  View fluentd logs:    tail -f /var/log/fluent/fluentd.log"
echo "  Restart collector:    sudo systemctl restart office365-collector"
echo "  Restart fluentd:      sudo systemctl restart fluentd"
echo "=========================================="
