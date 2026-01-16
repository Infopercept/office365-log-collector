# Office365 Audit Log Collector

A high-performance, production-ready Rust application that collects audit logs from Microsoft Office365 and delivers them to your SIEM or log management system in real-time.

**Built with Rust** | **Production-Ready** | **Zero Duplicates** | **Multi-Tenant** | **Standalone Service**

## Quick Start (Non-Developers)

### Download Pre-built Binary

Download the latest release for your platform from the [Releases](https://github.com/ddbnl/office365-audit-log-collector/releases) page:

- **Linux x86_64**: `office_audit_log_collector-linux-x86_64`
- **Linux ARM64**: `office_audit_log_collector-linux-arm64`
- **macOS**: `office_audit_log_collector-darwin`
- **Windows**: `office_audit_log_collector-windows.exe`

### Install as Systemd Service (Linux)

```bash
# 1. Download binary
wget https://github.com/ddbnl/office365-audit-log-collector/releases/latest/download/office_audit_log_collector-linux-x86_64
chmod +x office_audit_log_collector-linux-x86_64
sudo mv office_audit_log_collector-linux-x86_64 /usr/local/bin/office_audit_log_collector

# 2. Create directories
sudo mkdir -p /etc/office365-collector
sudo mkdir -p /var/lib/office365-collector
sudo mkdir -p /var/log/office365

# 3. Create config file (see Configuration section below)
sudo nano /etc/office365-collector/config.yaml

# 4. Create systemd service
sudo tee /etc/systemd/system/office365-collector.service << 'EOF'
[Unit]
Description=Office365 Audit Log Collector
After=network.target

[Service]
Type=simple
User=ubuntu
Group=ubuntu
WorkingDirectory=/var/lib/office365-collector
ExecStart=/usr/local/bin/office_audit_log_collector --config /etc/office365-collector/config.yaml
Restart=always
RestartSec=10

# Security hardening
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/office365-collector /var/log/office365

[Install]
WantedBy=multi-user.target
EOF

# 5. Enable and start
sudo systemctl daemon-reload
sudo systemctl enable office365-collector
sudo systemctl start office365-collector

# 6. Check status
sudo systemctl status office365-collector
sudo journalctl -u office365-collector -f
```

---

## Configuration

### Minimal Configuration (File Output - Recommended)

Create `/etc/office365-collector/config.yaml`:

```yaml
# Office365 Audit Log Collector Configuration
enabled: true

# Run continuously every 5 minutes
interval: "5m"

# Only collect new events (recommended for production)
only_future_events: true

# State files location
workingDir: "/var/lib/office365-collector"

# Office365 Credentials (from Azure AD App Registration)
tenants:
  - tenant_id: "YOUR-TENANT-ID"
    client_id: "YOUR-CLIENT-ID"
    client_secret: "YOUR-CLIENT-SECRET"
    api_type: "commercial"  # commercial, gcc, or gcc-high

# Subscribe to audit feeds
subscriptions:
  - "Audit.AzureActiveDirectory"
  - "Audit.Exchange"
  - "Audit.SharePoint"
  - "Audit.General"
  - "DLP.All"

# Output: Write to JSON files (simplest option)
output:
  file:
    path: "/var/log/office365/audit.json"
    separateByContentType: true

# Logging
log:
  path: ""  # Empty = stdout (for journalctl)
  debug: false
```

### Output Files

With `separateByContentType: true`, logs are written to separate files:

```
/var/log/office365/
├── AuditAzureActiveDirectory.json  # User logins, admin changes
├── AuditExchange.json              # Email operations
├── AuditSharePoint.json            # File operations
├── AuditGeneral.json               # Teams, PowerBI, etc.
└── DLPAll.json                     # Data Loss Prevention events
```

Each file is in **JSONL format** (one JSON object per line), compatible with:
- Filebeat → Elasticsearch
- Vector → Kafka/Clickhouse/etc.
- Promtail → Loki
- Logstash
- Any log shipper that reads JSON

---

## Available Output Options

| Output | Use Case | Configuration |
|--------|----------|---------------|
| **File** (Default) | Standalone service, read by log shipper | `output.file` |
| **Graylog** | Direct GELF output to Graylog | `output.graylog` |
| **Fluentd** | Stream to Fluentd/Vector via forward protocol | `output.fluentd` |
| **Azure Log Analytics** | Send to Azure Sentinel/OMS | `output.azureLogAnalytics` |

### File Output (Recommended)
```yaml
output:
  file:
    path: "/var/log/office365/audit.json"
    separateByContentType: true
```

### Fluentd Output
```yaml
output:
  fluentd:
    tenantName: "MyCompany"
    address: "fluentd-host"
    port: 24224
```

### Graylog Output
```yaml
output:
  graylog:
    address: "graylog-host"
    port: 12201
```

### Azure Log Analytics
```yaml
output:
  azureLogAnalytics:
    workspaceId: "YOUR-WORKSPACE-ID"
# Also requires --oms-key command line argument
```

---

## Azure AD Setup (Prerequisites)

### 1. Enable Audit Logging
- Microsoft 365 Compliance → Audit → Turn on auditing
- Wait 1-2 hours for audit pipeline to activate

### 2. Create App Registration
- Azure Portal → Azure AD → App registrations → New registration
- Name: `Office365-Log-Collector`
- Account type: Single tenant
- Save **Tenant ID** and **Client ID**

### 3. Create Client Secret
- Certificates & secrets → New client secret
- Save the **Secret Value** (shown only once!)

### 4. Add API Permissions
- API permissions → Add permission → Office 365 Management APIs
- Application permissions:
  - `ActivityFeed.Read`
  - `ActivityFeed.ReadDlp`
- Click **Grant admin consent**

---

## Multi-Tenant Configuration

Collect from multiple Office365 tenants:

```yaml
tenants:
  - tenant_id: "tenant1-id"
    client_id: "app1-client-id"
    client_secret: "app1-secret"
    api_type: "commercial"

  - tenant_id: "tenant2-id"
    client_id: "app2-client-id"
    client_secret: "app2-secret"
    api_type: "gcc"  # Government cloud
```

---

## Log Rotation

Configure logrotate for file output:

```bash
sudo tee /etc/logrotate.d/office365 << 'EOF'
/var/log/office365/*.json {
    daily
    rotate 7
    compress
    delaycompress
    notifempty
    missingok
    create 0644 ubuntu ubuntu
}
EOF
```

---

## Monitoring

### Check Service Status
```bash
sudo systemctl status office365-collector
sudo journalctl -u office365-collector -f
```

### Verify Log Collection
```bash
# Watch file sizes grow
watch -n 10 'ls -lh /var/log/office365/*.json'

# Count logs
wc -l /var/log/office365/*.json

# View recent logs
tail -5 /var/log/office365/AuditAzureActiveDirectory.json | jq .
```

### Expected Output
```
[INFO] Starting Office365 collector in daemon mode with interval: 300s
[INFO] Loaded 21428 known blobs into LRU cache
[INFO] Done! Blobs found: 9 | Blobs successful: 9 | Logs saved: 2063
[INFO] Sleeping for 300 seconds until next collection...
```

---

## Docker Deployment

```bash
docker run -d \
  --name office365-collector \
  --restart unless-stopped \
  -v $(pwd)/config.yaml:/app/config.yaml:ro \
  -v office365-state:/var/lib/office365-collector \
  -v office365-logs:/var/log/office365 \
  ghcr.io/ddbnl/office365-audit-log-collector:latest
```

---

## Building from Source

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/ddbnl/office365-audit-log-collector.git
cd office365-audit-log-collector
cargo build --release

# Binary at: ./target/release/office_audit_log_collector
```

---

## Troubleshooting

### No Logs Collected
1. Check audit logging is enabled in Office365
2. Verify API permissions (ActivityFeed.Read, ActivityFeed.ReadDlp)
3. Ensure admin consent was granted
4. Check credentials in config.yaml

### Authentication Errors
- Verify tenant_id, client_id, client_secret
- Regenerate client secret if expired
- Check API permissions

### High Memory Usage
Reduce settings in config:
```yaml
collect:
  cacheSize: 100000  # Default: 500000
  maxThreads: 25     # Default: 50
```

---

## Key Features

- **Multi-Tenant Support** - Collect from multiple Office365 tenants
- **5 Subscription Types** - DLP, Exchange, SharePoint, Azure AD, General
- **Daemon Mode** - Runs continuously (configurable interval)
- **Zero Duplicates** - LRU cache with TTL-based blob tracking
- **Multiple Outputs** - File, Fluentd, Graylog, Azure Log Analytics
- **Government Cloud** - Commercial, GCC, GCC-High support
- **Memory Efficient** - Bounded caches, chunked processing
- **Automatic Retry** - Handles API failures gracefully

---

## License

See LICENSE.md

## Credits

Originally created by [ddbnl](https://github.com/ddbnl). Rust rewrite and ongoing maintenance.
