# Office365 Audit Log Collector - Deployment Guide

## Prerequisites

- Linux server (Ubuntu 20.04+ recommended)
- Rust toolchain (for building) or pre-built binary
- Azure AD App Registration with Office365 Management API permissions
- Fluentd (optional, for log forwarding)

## Quick Start

### 1. Build the Collector

```bash
cd /home/ubuntu/new-siem/office365-audit-log-collector
cargo build --release
```

Binary location: `target/release/office_audit_log_collector`

### 2. Create Configuration

```bash
mkdir -p config
cat > config/config.yaml << 'EOF'
enabled: true
interval: "5m"
only_future_events: true

tenants:
  - tenant_id: "YOUR-TENANT-ID"
    client_id: "YOUR-CLIENT-ID"
    client_secret: "YOUR-CLIENT-SECRET"
    api_type: "commercial"

subscriptions:
  - "Audit.AzureActiveDirectory"
  - "Audit.Exchange"
  - "Audit.SharePoint"
  - "Audit.General"
  - "DLP.All"

output:
  fluentd:
    tenantName: "YourOrg"
    address: "localhost"
    port: 24224

log:
  path: ""
  debug: false
EOF
```

### 3. Create Systemd Service

```bash
sudo tee /etc/systemd/system/office365-collector.service << 'EOF'
[Unit]
Description=Office365 Audit Log Collector
After=network.target fluentd.service

[Service]
Type=simple
User=ubuntu
WorkingDirectory=/home/ubuntu/new-siem/office365-audit-log-collector
ExecStart=/home/ubuntu/new-siem/office365-audit-log-collector/target/release/office_audit_log_collector --config /home/ubuntu/new-siem/office365-audit-log-collector/config/config.yaml
Restart=always
RestartSec=10
LimitNOFILE=65535

StandardOutput=journal
StandardError=journal
SyslogIdentifier=office365-collector

[Install]
WantedBy=multi-user.target
EOF
```

### 4. Start the Service

```bash
sudo systemctl daemon-reload
sudo systemctl enable office365-collector
sudo systemctl start office365-collector
```

### 5. Verify

```bash
# Check service status
sudo systemctl status office365-collector

# View logs
journalctl -u office365-collector -f
```

## Fluentd Setup

### Install Fluentd

```bash
# Ubuntu/Debian
curl -fsSL https://toolbelt.treasuredata.com/sh/install-ubuntu-noble-fluent-package5-lts.sh | sh

# Enable and start
sudo systemctl enable fluentd
sudo systemctl start fluentd
```

### Configure Fluentd

Create `/etc/fluent/fluentd.conf`:

```xml
<source>
  @type forward
  port 24224
  bind 0.0.0.0
</source>

# Route by subscription type
<match YourOrg>
  @type rewrite_tag_filter
  <rule>
    key OriginFeed
    pattern /^Audit\.AzureActiveDirectory$/
    tag office365.AuditAzureActiveDirectory
  </rule>
  <rule>
    key OriginFeed
    pattern /^Audit\.Exchange$/
    tag office365.AuditExchange
  </rule>
  <rule>
    key OriginFeed
    pattern /^Audit\.SharePoint$/
    tag office365.AuditSharePoint
  </rule>
  <rule>
    key OriginFeed
    pattern /^Audit\.General$/
    tag office365.AuditGeneral
  </rule>
  <rule>
    key OriginFeed
    pattern /^DLP\.All$/
    tag office365.DLPAll
  </rule>
  <rule>
    key OriginFeed
    pattern /.+/
    tag office365.Other
  </rule>
</match>

# Write to separate files per subscription
<match office365.AuditAzureActiveDirectory>
  @type file
  path /var/log/fluent/office365/AuditAzureActiveDirectory
  <format>
    @type json
  </format>
  <buffer>
    flush_interval 10s
  </buffer>
  append true
</match>

<match office365.AuditExchange>
  @type file
  path /var/log/fluent/office365/AuditExchange
  <format>
    @type json
  </format>
  <buffer>
    flush_interval 10s
  </buffer>
  append true
</match>

<match office365.AuditSharePoint>
  @type file
  path /var/log/fluent/office365/AuditSharePoint
  <format>
    @type json
  </format>
  <buffer>
    flush_interval 10s
  </buffer>
  append true
</match>

<match office365.AuditGeneral>
  @type file
  path /var/log/fluent/office365/AuditGeneral
  <format>
    @type json
  </format>
  <buffer>
    flush_interval 10s
  </buffer>
  append true
</match>

<match office365.DLPAll>
  @type file
  path /var/log/fluent/office365/DLPAll
  <format>
    @type json
  </format>
  <buffer>
    flush_interval 10s
  </buffer>
  append true
</match>

<match office365.**>
  @type file
  path /var/log/fluent/office365/Other
  <format>
    @type json
  </format>
  <buffer>
    flush_interval 10s
  </buffer>
  append true
</match>
```

### Create Output Directory

```bash
sudo mkdir -p /var/log/fluent/office365
sudo chown -R _fluentd:_fluentd /var/log/fluent/office365
sudo systemctl restart fluentd
```

## Remote Server Deployment

### Deploy to Remote Server

```bash
# Build locally
cargo build --release

# Copy binary to remote
scp -i your-key.pem target/release/office_audit_log_collector ubuntu@REMOTE_IP:/home/ubuntu/office365-audit-log-collector/

# Copy config
scp -i your-key.pem config/config.yaml ubuntu@REMOTE_IP:/home/ubuntu/office365-audit-log-collector/config/

# SSH and setup service
ssh -i your-key.pem ubuntu@REMOTE_IP
```

Then on remote server, create systemd service as shown above.

### Update Remote Server

```bash
# Stop service
ssh -i your-key.pem ubuntu@REMOTE_IP "sudo systemctl stop office365-collector"

# Copy new binary
scp -i your-key.pem target/release/office_audit_log_collector ubuntu@REMOTE_IP:/home/ubuntu/office365-audit-log-collector/

# Start service
ssh -i your-key.pem ubuntu@REMOTE_IP "sudo systemctl start office365-collector"
```

## Directory Structure

```
/home/ubuntu/new-siem/office365-audit-log-collector/
├── target/release/
│   └── office_audit_log_collector    # Binary
├── config/
│   └── config.yaml                   # Configuration
├── src/                              # Source code
├── docs/
│   ├── CONFIGURATION.md              # Config reference
│   └── DEPLOYMENT.md                 # This file
├── office365-*.json                  # State files (auto-created)
└── known_blobs                       # Blob tracking (auto-created)

/var/log/fluent/office365/            # Fluentd output
├── AuditAzureActiveDirectory.json
├── AuditExchange.json
├── AuditSharePoint.json
├── AuditGeneral.json
└── DLPAll.json
```

## Monitoring

### Check Service Status

```bash
sudo systemctl status office365-collector
```

### View Logs

```bash
# Real-time logs
journalctl -u office365-collector -f

# Last 100 lines
journalctl -u office365-collector -n 100

# Errors only
journalctl -u office365-collector -p err
```

### Check Collection Stats

Logs show per-run statistics:
```
Blobs found: 23
Blobs successful: 23
Blobs failed: 0
Blobs retried: 0
Logs saved: 6163
```

### Check State Files

```bash
cat office365-*.json
```

### Check Output Files

```bash
# File counts
wc -l /var/log/fluent/office365/*.json

# Latest entries
tail -1 /var/log/fluent/office365/AuditAzureActiveDirectory.json | jq .
```

## Troubleshooting

### Service Won't Start

```bash
# Check systemd logs
journalctl -u office365-collector -n 50

# Test manually
./target/release/office_audit_log_collector --config config/config.yaml
```

### No Logs Collected

1. Check credentials in config
2. Verify Azure AD permissions
3. Check Fluentd is running: `systemctl status fluentd`
4. Check state files exist

### API Errors

| Error | Cause | Solution |
|-------|-------|----------|
| `401 Unauthorized` | Invalid credentials | Check tenant_id, client_id, client_secret |
| `403 Forbidden` | Missing permissions | Grant ActivityFeed.Read in Azure AD |
| `AF20055` | Invalid time range | Delete state files and restart |

### Reset Collection

To re-collect from scratch:
```bash
sudo systemctl stop office365-collector
rm -f office365-*.json known_blobs
sudo systemctl start office365-collector
```

### High Memory Usage

Normal: ~200MB during collection, ~100MB idle

If higher:
1. Reduce subscriptions
2. Increase interval
3. Check for stuck retries in logs

## Service Management

```bash
# Start
sudo systemctl start office365-collector

# Stop
sudo systemctl stop office365-collector

# Restart
sudo systemctl restart office365-collector

# Enable on boot
sudo systemctl enable office365-collector

# Disable on boot
sudo systemctl disable office365-collector

# View status
sudo systemctl status office365-collector
```

## Security Recommendations

1. **Protect config file** (contains secrets):
   ```bash
   chmod 600 config/config.yaml
   ```

2. **Use secret file** instead of inline secret:
   ```yaml
   tenants:
     - tenant_id: "..."
       client_id: "..."
       client_secret_path: "/etc/secrets/office365.txt"
   ```

3. **Restrict log access**:
   ```bash
   chmod 750 /var/log/fluent/office365
   ```

4. **Rotate credentials** periodically in Azure AD

## Performance

- **Collection speed**: ~30,000 logs/second
- **Memory**: 100-200MB typical
- **CPU**: Minimal (mostly I/O bound)
- **Network**: Outbound HTTPS to Microsoft only
