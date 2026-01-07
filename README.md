# Office365 Audit Log Collector

A high-performance, production-ready Rust application that collects audit logs from Microsoft Office365 and delivers them to your SIEM or log management system in real-time.

**Built with Rust** 🦀 | **Production-Ready** ✅ | **Zero Duplicates** 🎯 | **Multi-Tenant** 🏢

## 📋 Table of Contents

- [What It Does](#what-it-does)
- [Key Features](#key-features)
- [How It Works](#how-it-works)
- [Quick Start](#quick-start)
- [Configuration](#configuration)
- [Deployment](#deployment)
- [Production Setup](#production-setup)
- [Monitoring](#monitoring)
- [Troubleshooting](#troubleshooting)

---

## What It Does

Continuously collects audit logs from Microsoft Office365 (Exchange, SharePoint, Azure AD, Teams, DLP events, etc.) and streams them to your SIEM in real-time.

### Use Cases:
- **Security Monitoring**: Detect suspicious activities, unauthorized access, data exfiltration
- **Compliance**: Maintain audit trails for regulatory requirements (SOC2, HIPAA, GDPR)
- **Incident Response**: Investigate security incidents with complete Office365 activity logs
- **DLP Monitoring**: Track Data Loss Prevention policy violations
- **User Activity Tracking**: Monitor file access, email activity, login events

---

## Key Features

✅ **Multi-Tenant Support** - Collect logs from multiple Office365 tenants simultaneously
✅ **5 Subscription Types** - DLP, Exchange, SharePoint, Azure AD, General (Teams, PowerBI, etc.)
✅ **Daemon Mode** - Runs continuously, collects logs every 5 minutes (configurable)
✅ **JSON Output** - Native JSON format, with option for separate files per subscription
✅ **Zero Duplicates** - Intelligent blob tracking prevents duplicate log collection
✅ **Fluentd Integration** - Stream logs directly to Fluentd/Vector/SIEM
✅ **File Output** - Alternative output to JSON files with automatic append
✅ **Government Cloud Support** - Works with Commercial, GCC, and GCC-High clouds
✅ **Automatic Retry** - Handles API failures and rate limiting gracefully
✅ **Dockerized** - Ready-to-deploy Docker container

---

## How It Works

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    PRODUCTION FLOW                           │
└─────────────────────────────────────────────────────────────┘

Office365 Management API
         │
         │ Every 5 minutes (configurable)
         ↓
┌─────────────────────┐
│  O365 Collector     │  ← Rust daemon (this application)
│  (Daemon Mode)      │
└─────────────────────┘
         │
         ├─→ Fluentd :24224 ──→ Vector ──→ Kafka ──→ SIEM
         │                        or
         └─→ JSON Files ──→ Filebeat/Vector ──→ SIEM
```

### Collection Process

1. **Daemon Starts**
   - Loads configuration
   - Authenticates with Office365 Management API
   - Subscribes to configured audit feeds (DLP, Exchange, SharePoint, etc.)
   - Loads `known_blobs` state file (tracks processed logs)

2. **Every Interval (default: 5 minutes)**
   - Queries Office365 API for available log blobs (batches)
   - Filters out already-processed blobs (using `known_blobs`)
   - Downloads ONLY new blobs
   - Parses JSON logs from blobs
   - Sends logs to configured output (Fluentd or files)
   - Updates `known_blobs` state file
   - Sleeps until next interval

3. **Deduplication**
   - Each Office365 log blob has a unique ID
   - Collector tracks processed blob IDs in `known_blobs` file
   - Even if you rotate/delete output files, no duplicates will occur
   - Blob IDs persist across restarts

### Available Subscription Types

| Subscription | Description | Typical Log Volume |
|--------------|-------------|-------------------|
| **DLP.All** | Data Loss Prevention policy matches, sensitive data detection | Medium |
| **Audit.Exchange** | Email operations, mailbox access, mail submission | High |
| **Audit.SharePoint** | File operations, sharing, OneDrive activity | Very High |
| **Audit.AzureActiveDirectory** | User logins, admin changes, MFA events | Medium |
| **Audit.General** | Teams, PowerBI, Forms, Yammer, Dynamics, etc. | Medium |

---

## Quick Start

### Prerequisites

1. **Office365 Tenant with Audit Logging Enabled**
   - Go to Microsoft 365 Compliance → Audit → Turn on auditing
   - Wait 1-2 hours for audit pipeline to activate

2. **Azure AD App Registration**
   - Azure Portal → Azure Active Directory → App registrations → New registration
   - Name: `Office365-Log-Collector`
   - Supported account types: **Single tenant**
   - Click **Register**
   - Save the **Tenant ID** and **Client ID**

3. **Client Secret**
   - In your app registration → Certificates & secrets → New client secret
   - Description: `collector-secret`
   - Expiry: Choose appropriate duration
   - Save the **Secret Value** (shown only once!)

4. **API Permissions**
   - In your app registration → API permissions → Add a permission
   - Select **Office 365 Management APIs** → Application permissions
   - Add these permissions:
     - ✅ `ActivityFeed.Read`
     - ✅ `ActivityFeed.ReadDlp`
   - Click **Grant admin consent** (requires admin)

### Installation

#### Option 1: Docker (Recommended)

```bash
# Pull the image
docker pull ghcr.io/ddbnl/office365-audit-log-collector:latest

# Or build locally
docker build -f docker/Dockerfile -t office365-collector:latest .
```

#### Option 2: Bare Metal

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone repository
git clone https://github.com/ddbnl/office365-audit-log-collector.git
cd office365-audit-log-collector

# Build release binary
cargo build --release

# Binary will be at: ./target/release/office_audit_log_collector
```

---

## Configuration

### Basic Configuration

Create `config/config.yaml`:

```yaml
# Enable the collector
enabled: true

# DAEMON MODE: Run continuously with 5-minute intervals
interval: "5m"

# Only collect NEW events (recommended for production)
only_future_events: true

# Office365 Credentials
tenants:
  - tenant_id: "YOUR-TENANT-ID"
    client_id: "YOUR-CLIENT-ID"
    client_secret: "YOUR-CLIENT-SECRET"
    api_type: "commercial"  # Options: commercial, gcc, gcc-high

# Subscribe to all audit feeds
subscriptions:
  - "DLP.All"
  - "Audit.Exchange"
  - "Audit.SharePoint"
  - "Audit.AzureActiveDirectory"
  - "Audit.General"

# Output: Send to Fluentd
output:
  fluentd:
    enabled: true
    tenantName: "MyCompany"
    address: "localhost"  # or "fluentd" if using Docker
    port: 24224

# Logging
log:
  path: "/var/log/office365-collector.log"
  debug: false
```

### File Output Configuration

If you prefer file output instead of Fluentd:

```yaml
output:
  file:
    path: "/var/log/office365/audit.json"
    separateByContentType: true  # Creates 5 separate files
```

**Output files (with `separateByContentType: true`):**
- `DLPAll.json` - DLP policy violations
- `AuditExchange.json` - Email activity
- `AuditSharePoint.json` - File operations
- `AuditAzureActiveDirectory.json` - Login events
- `AuditGeneral.json` - Teams, PowerBI, etc.

**File behavior:**
- Dateless filenames (simple names)
- Collector appends new data every interval
- You handle rotation (logrotate, manual, etc.)
- No duplicates even after rotation

### Multi-Tenant Configuration

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

### Advanced Configuration

```yaml
# Collection settings
collect:
  workingDir: "/var/lib/office365-collector"  # State files location
  cacheSize: 500000                            # In-memory log cache
  maxThreads: 50                               # Concurrent downloads
  retries: 3                                   # Retry failed downloads
  skipKnownLogs: true                          # Skip processed blobs
  hoursToCollect: 24                           # Look back 24 hours

# Interval options
interval: "5m"   # Every 5 minutes (recommended)
# interval: "1m"   # Every 1 minute (high-volume)
# interval: "15m"  # Every 15 minutes (low-volume)
# interval: "1h"   # Every hour

# API settings
curl_max_size: "10M"  # Max response size from Office365 API
```

### Configuration File Locations

```
config/
├── config.yaml                        # Main config (production)
├── config.production.template.yaml    # Template with all options
├── config-test.yaml                   # Test configuration
└── credentials.yaml.example           # Credentials example
```

---

## Deployment

### Docker Deployment (Recommended)

#### Using Docker Compose

Create `docker-compose.yml`:

```yaml
version: '3.8'

services:
  office365-collector:
    image: office365-collector:latest
    container_name: office365-collector
    restart: unless-stopped
    volumes:
      # Config (read-only)
      - ./config/config.yaml:/app/config.yaml:ro

      # State persistence (known_blobs)
      - office365-state:/var/lib/office365-collector

      # Output files (if using file output)
      - office365-logs:/var/log/office365

    environment:
      - TZ=UTC

    # If using Fluentd
    depends_on:
      - fluentd
    networks:
      - siem-network

  # Optional: Fluentd service
  fluentd:
    image: fluent/fluentd:latest
    container_name: fluentd
    ports:
      - "24224:24224"
    volumes:
      - ./fluentd/fluent.conf:/fluentd/etc/fluent.conf:ro
    networks:
      - siem-network

volumes:
  office365-state:
  office365-logs:

networks:
  siem-network:
    driver: bridge
```

**Start the collector:**

```bash
docker-compose up -d
```

**View logs:**

```bash
docker-compose logs -f office365-collector
```

#### Using Docker Run

```bash
docker run -d \
  --name office365-collector \
  --restart unless-stopped \
  -v $(pwd)/config/config.yaml:/app/config.yaml:ro \
  -v office365-state:/var/lib/office365-collector \
  -v office365-logs:/var/log/office365 \
  office365-collector:latest
```

### Bare Metal Deployment

#### 1. Build and Install

```bash
# Build release binary
cargo build --release

# Install binary
sudo cp target/release/office_audit_log_collector /usr/local/bin/

# Create directories
sudo mkdir -p /etc/office365-collector
sudo mkdir -p /var/lib/office365-collector
sudo mkdir -p /var/log/office365

# Copy config
sudo cp config/config.yaml /etc/office365-collector/config.yaml

# Set permissions
sudo chown -R $USER:$USER /var/lib/office365-collector
sudo chown -R $USER:$USER /var/log/office365
```

#### 2. Create Systemd Service

Create `/etc/systemd/system/office365-collector.service`:

```ini
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
```

#### 3. Enable and Start

```bash
# Reload systemd
sudo systemctl daemon-reload

# Enable service (start on boot)
sudo systemctl enable office365-collector

# Start service
sudo systemctl start office365-collector

# Check status
sudo systemctl status office365-collector

# View logs
sudo journalctl -u office365-collector -f
```

---

## Production Setup

### Log Rotation (File Output)

If using file output, configure logrotate:

Create `/etc/logrotate.d/office365`:

```
/var/log/office365/*.json {
    daily
    rotate 7
    compress
    delaycompress
    notifempty
    missingok
    create 0644 ubuntu ubuntu
    sharedscripts
    postrotate
        # Optional: send SIGHUP to collector if needed
    endscript
}
```

Test rotation:

```bash
sudo logrotate -f /etc/logrotate.d/office365
```

### Fluentd Integration

Example Fluentd configuration (`fluentd/fluent.conf`):

```ruby
<source>
  @type forward
  port 24224
  bind 0.0.0.0
</source>

<filter office365.**>
  @type record_transformer
  <record>
    source office365
    collector_host "#{Socket.gethostname}"
  </record>
</filter>

<match office365.**>
  @type forward
  <server>
    host vector
    port 8686
  </server>
  <buffer>
    @type file
    path /var/log/fluentd/office365-buffer
    flush_interval 10s
  </buffer>
</match>
```

### Vector Integration (Alternative to Fluentd)

If using Vector to read JSON files:

```toml
[sources.office365_files]
type = "file"
include = ["/var/log/office365/*.json"]
read_from = "end"

[transforms.parse_office365]
type = "remap"
inputs = ["office365_files"]
source = '''
. = parse_json!(.message)
.source = "office365"
'''

[sinks.kafka]
type = "kafka"
inputs = ["parse_office365"]
bootstrap_servers = "kafka:9092"
topic = "office365-logs"
encoding.codec = "json"
```

### State Management

**Important Files:**

```
/var/lib/office365-collector/
└── known_blobs                 # Tracks processed log blobs
                                # KEEP THIS FILE PERSISTENT!
```

**Docker Volume:**
```bash
# Backup state
docker run --rm -v office365-state:/data -v $(pwd):/backup \
  ubuntu tar czf /backup/office365-state.tar.gz -C /data .

# Restore state
docker run --rm -v office365-state:/data -v $(pwd):/backup \
  ubuntu tar xzf /backup/office365-state.tar.gz -C /data
```

---

## Monitoring

### Health Checks

#### 1. Check Service Status

**Docker:**
```bash
docker ps | grep office365-collector
docker logs office365-collector --tail 50
```

**Systemd:**
```bash
sudo systemctl status office365-collector
sudo journalctl -u office365-collector --since "1 hour ago"
```

#### 2. Verify Log Collection

**Look for successful collections:**
```bash
# Docker
docker logs office365-collector | grep "Done!"

# Systemd
sudo journalctl -u office365-collector | grep "Done!"
```

**Expected output:**
```
[INFO] Done! Blobs found: 50 | Blobs successful: 50 | Logs saved: 25000
```

#### 3. Monitor Output

**Fluentd output:**
```bash
# Check Fluentd received logs
docker logs fluentd | grep "office365"
```

**File output:**
```bash
# Watch file sizes grow
watch -n 10 'ls -lh /var/log/office365/*.json'

# Count logs
wc -l /var/log/office365/*.json
```

### Key Metrics to Monitor

| Metric | What to Check | Alert On |
|--------|---------------|----------|
| **Blobs Found** | Number of log batches available | Sudden drop to 0 |
| **Blobs Failed** | Failed downloads | > 0 for multiple cycles |
| **Logs Saved** | Number of logs collected | Sudden drop or 0 |
| **Collector Uptime** | Service running continuously | Service stopped |
| **File Sizes** | Output files growing | Files not growing |
| **known_blobs** | State file exists and updates | File missing or stale |

### Common Log Messages

**✅ Normal Operation:**
```
[INFO] Starting Office365 collector in daemon mode with interval: 300s
[INFO] Successfully logged in to Office Management API
[INFO] Already subscribed to feed dlp.all
[INFO] Spawned collector threads
[INFO] Done! Blobs found: 50 | Blobs successful: 50 | Logs saved: 25000
[INFO] Sleeping for 300 seconds until next collection...
```

**⚠️ Warnings (Usually OK):**
```
[WARN] No new blobs found (this is normal if no activity)
[WARN] Retrying blob download (transient network issue)
```

**❌ Errors (Needs Attention):**
```
[ERROR] Authentication failed (check credentials)
[ERROR] Subscription failed (check API permissions)
[ERROR] Blob download failed (check network/API availability)
```

---

## Troubleshooting

### No Logs Being Collected

**Checklist:**

1. ✅ **Audit logging enabled in Office365?**
   ```bash
   # Verify in Microsoft 365 Compliance portal
   # Audit → Turn on auditing
   ```

2. ✅ **Correct API permissions granted?**
   - Azure AD → App registrations → Your app → API permissions
   - Must have: `ActivityFeed.Read`, `ActivityFeed.ReadDlp`
   - Admin consent granted?

3. ✅ **Credentials correct?**
   ```bash
   # Check config file
   cat config/config.yaml | grep -A5 tenants
   ```

4. ✅ **Collector running?**
   ```bash
   # Docker
   docker ps | grep office365-collector

   # Systemd
   sudo systemctl status office365-collector
   ```

5. ✅ **Check collector logs for errors:**
   ```bash
   # Docker
   docker logs office365-collector --tail 100

   # Systemd
   sudo journalctl -u office365-collector -n 100
   ```

### Authentication Errors

**Error:** `Authentication failed`

**Fix:**
1. Verify tenant_id, client_id, client_secret in config
2. Check API permissions in Azure AD
3. Ensure admin consent was granted
4. Try regenerating client secret

### API Rate Limiting

**Error:** `Too many requests`

**Fix:**
- Increase `interval` in config (e.g., from `5m` to `10m`)
- Reduce `maxThreads` (default: 50, try 25)
- Office365 API has rate limits, collector will auto-retry

### Missing Logs / Gaps

**Check:**

1. **Office365 API delay:**
   - Logs appear 5-60 minutes after events occur
   - Normal behavior, not a collector issue

2. **known_blobs file:**
   ```bash
   # Check if file exists and is updating
   ls -lh /var/lib/office365-collector/known_blobs

   # View recent entries
   tail -20 /var/lib/office365-collector/known_blobs
   ```

3. **Collector uptime:**
   - If collector was down, it will catch up on next run
   - Office365 keeps blobs available for 7 days

### Duplicate Logs

**Possible causes:**

1. **Deleted known_blobs file:**
   - Collector will re-download all available blobs
   - Solution: Keep known_blobs persistent

2. **Multiple collector instances:**
   - Each needs its own known_blobs file
   - Or use same known_blobs (shared volume/filesystem)

3. **skipKnownLogs disabled:**
   ```yaml
   collect:
     skipKnownLogs: true  # Must be true (default)
   ```

### High Memory Usage

**Fix:**

Reduce cache size in config:

```yaml
collect:
  cacheSize: 100000  # Default: 500000
  maxThreads: 25     # Default: 50
```

### File Output Not Working

**Check:**

1. **Directory exists and writable:**
   ```bash
   ls -ld /var/log/office365
   # Should be owned by collector user
   ```

2. **Config syntax:**
   ```yaml
   output:
     file:
       path: "/var/log/office365/audit.json"
       separateByContentType: true  # Note: camelCase!
   ```

3. **Permissions:**
   ```bash
   sudo chown -R ubuntu:ubuntu /var/log/office365
   sudo chmod 755 /var/log/office365
   ```

---

## Configuration Reference

### Full Configuration Example

See `config/config.production.template.yaml` for all available options.

### Environment Variables

You can override config values with environment variables:

```bash
export TENANT_ID="your-tenant-id"
export CLIENT_ID="your-client-id"
export CLIENT_SECRET="your-secret"

# Run collector
./office_audit_log_collector --config config.yaml
```

### Command Line Options

```bash
# Run with custom config
./office_audit_log_collector --config /path/to/config.yaml

# Single-run mode (for testing)
# Set only_future_events: false in config
./office_audit_log_collector --config config-test.yaml
```

---

## Additional Resources

- **Documentation:** `docs/` directory
  - `docs/PRODUCTION-FLOW.md` - Detailed production flow
  - `docs/CREDENTIALS-CHECKLIST.md` - Setup guide
  - `docs/DOCKER-DEPLOYMENT.md` - Docker specifics

- **Microsoft Documentation:**
  - [Office 365 Management Activity API](https://docs.microsoft.com/en-us/office/office-365-management-api/office-365-management-activity-api-reference)
  - [Enable Audit Logging](https://docs.microsoft.com/en-us/microsoft-365/compliance/turn-audit-log-search-on-or-off)

- **Support:**
  - [GitHub Issues](https://github.com/ddbnl/office365-audit-log-collector/issues)
  - [Discussions](https://github.com/ddbnl/office365-audit-log-collector/discussions)

---

## Contributing

Contributions welcome! Please open an issue or pull request.

**Areas for contribution:**
- Additional output interfaces (Kafka, Elasticsearch, etc.)
- Performance improvements
- Documentation improvements
- Bug fixes

---

## License

See LICENSE.md

---

## Credits

Originally created by [ddbnl](https://github.com/ddbnl). Rust rewrite and ongoing maintenance.

**Built with:**
- Rust 🦀
- tokio (async runtime)
- reqwest (HTTP client)
- serde (JSON parsing)
