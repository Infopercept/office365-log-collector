# Office365 Audit Log Collector - Configuration Guide

## Overview

The Office365 Audit Log Collector fetches audit logs from Microsoft Office365 Management Activity API and forwards them to various outputs (Fluentd, file, Graylog, Azure Log Analytics).

## Configuration File

Create a YAML configuration file (e.g., `config.yaml`):

```yaml
# Enable/disable the collector
enabled: true

# Collection interval (daemon mode)
# Formats: "30s", "5m", "1h", "1d"
interval: "5m"

# Only collect new events (skip historical logs on first run)
# true = Start from NOW on first deployment (recommended for production)
# false = Collect last 24 hours of historical logs
only_future_events: true

# Office365 tenants (supports multiple tenants)
tenants:
  - tenant_id: "your-tenant-id"
    client_id: "your-app-client-id"
    client_secret: "your-client-secret"
    api_type: "commercial"  # Options: commercial, gcc, gcc-high

# Subscriptions to collect
subscriptions:
  - "Audit.AzureActiveDirectory"  # Azure AD sign-ins, user/group changes
  - "Audit.Exchange"              # Email operations, mailbox access
  - "Audit.SharePoint"            # SharePoint/OneDrive file operations
  - "Audit.General"               # Teams, PowerBI, Forms, etc.
  - "DLP.All"                     # Data Loss Prevention events

# Output configuration (choose one or more)
output:
  # Option 1: Fluentd (recommended for SIEM pipelines)
  fluentd:
    tenantName: "YourOrgName"
    address: "localhost"
    port: 24224

  # Option 2: JSON file output
  file:
    path: "/var/logs/office365/audit.json"
    separateByContentType: true  # Creates separate files per subscription

  # Option 3: Graylog GELF
  graylog:
    address: "graylog.example.com"
    port: 12201

  # Option 4: Azure Log Analytics
  azureLogAnalytics:
    workspaceId: "your-workspace-id"
    # Pass shared key via --oms-key CLI argument

# Logging configuration
log:
  path: ""       # Empty = stdout (for systemd/docker)
  debug: false   # Set true for troubleshooting
```

## Configuration Options Explained

### `enabled`
- `true`: Collector runs normally
- `false`: Collector exits immediately (useful for maintenance)

### `interval`
Daemon mode collection interval. Supports:
- Seconds: `"30s"`, `"60s"`
- Minutes: `"1m"`, `"5m"`, `"10m"`
- Hours: `"1h"`, `"2h"`
- Days: `"1d"`

**Recommended:** `"5m"` for most deployments

### `only_future_events`
Controls first-run behavior:

| Value | First Run | Subsequent Runs |
|-------|-----------|-----------------|
| `true` | Collects from NOW (no historical) | Collects since last run (delta) |
| `false` | Collects last 24 hours | Collects since last run (delta) |

**Recommended:** `true` for production deployments

### `tenants`
Array of Office365 tenant configurations:

| Field | Description |
|-------|-------------|
| `tenant_id` | Azure AD tenant ID (GUID) |
| `client_id` | App registration client ID |
| `client_secret` | App registration client secret |
| `client_secret_path` | Alternative: path to file containing secret |
| `api_type` | `commercial` (default), `gcc`, or `gcc-high` |

**Multi-tenant example:**
```yaml
tenants:
  - tenant_id: "tenant-1-guid"
    client_id: "app-1-client-id"
    client_secret: "secret-1"
    api_type: "commercial"
  - tenant_id: "tenant-2-guid"
    client_id: "app-2-client-id"
    client_secret_path: "/etc/secrets/tenant2.txt"
    api_type: "gcc-high"
```

### `subscriptions`
List of Office365 audit feeds to collect:

| Subscription | Content |
|--------------|---------|
| `Audit.AzureActiveDirectory` | Azure AD sign-ins, user management, group changes |
| `Audit.Exchange` | Email operations, mailbox access, admin actions |
| `Audit.SharePoint` | SharePoint/OneDrive file operations, sharing |
| `Audit.General` | Teams, PowerBI, Forms, Yammer, etc. |
| `DLP.All` | Data Loss Prevention policy matches |

### `output`
Configure one or more output destinations:

#### Fluentd Output
```yaml
output:
  fluentd:
    tenantName: "OrgName"   # Tag prefix for Fluentd routing
    address: "localhost"    # Fluentd host
    port: 24224            # Fluentd forward port
```

#### File Output
```yaml
output:
  file:
    path: "/var/logs/office365/audit.json"
    separateByContentType: true  # Optional: separate files per subscription
```

When `separateByContentType: true`, creates:
- `AuditAzureActiveDirectory.json`
- `AuditExchange.json`
- `AuditSharePoint.json`
- `AuditGeneral.json`
- `DLPAll.json`

#### Graylog Output
```yaml
output:
  graylog:
    address: "graylog.example.com"
    port: 12201
```

#### Azure Log Analytics
```yaml
output:
  azureLogAnalytics:
    workspaceId: "workspace-guid"
```
Run with: `--oms-key "your-shared-key"`

## State Management

The collector maintains state files to track last collection time:
```
office365-{tenant_id}-{subscription}.json
```

Example:
```json
{
  "last_log_time": "2026-01-07T10:54:03.658865200Z",
  "last_run": "2026-01-07T10:54:03.658865200Z",
  "first_run": false
}
```

**Important:** Don't delete state files unless you want to reset collection.

## Environment Variables

| Variable | Description |
|----------|-------------|
| `RUST_LOG` | Log level: `error`, `warn`, `info`, `debug`, `trace` |

## CLI Arguments

```bash
office_audit_log_collector --config /path/to/config.yaml [OPTIONS]

Options:
  --config <PATH>       Path to YAML configuration file (required)
  --publisher-id <ID>   Publisher ID for API calls (optional)
  --oms-key <KEY>       Azure Log Analytics shared key (for azureLogAnalytics output)
  --interactive         Interactive mode (disabled in production)
```

## Example Configurations

### Minimal Production Config
```yaml
enabled: true
interval: "5m"
only_future_events: true

tenants:
  - tenant_id: "your-tenant-id"
    client_id: "your-client-id"
    client_secret: "your-secret"

subscriptions:
  - "Audit.AzureActiveDirectory"
  - "Audit.Exchange"
  - "Audit.SharePoint"
  - "Audit.General"
  - "DLP.All"

output:
  fluentd:
    tenantName: "MyOrg"
    address: "localhost"
    port: 24224
```

### Multi-Tenant with File Output
```yaml
enabled: true
interval: "2m"
only_future_events: true

tenants:
  - tenant_id: "tenant-1"
    client_id: "app-1"
    client_secret: "secret-1"
  - tenant_id: "tenant-2"
    client_id: "app-2"
    client_secret: "secret-2"

subscriptions:
  - "Audit.AzureActiveDirectory"
  - "Audit.Exchange"

output:
  file:
    path: "/var/logs/office365/audit.json"
    separateByContentType: true
```

### GCC-High Government Cloud
```yaml
enabled: true
interval: "5m"
only_future_events: true

tenants:
  - tenant_id: "gov-tenant-id"
    client_id: "gov-app-id"
    client_secret_path: "/etc/secrets/office365.txt"
    api_type: "gcc-high"

subscriptions:
  - "Audit.AzureActiveDirectory"
  - "Audit.Exchange"

output:
  fluentd:
    tenantName: "GovOrg"
    address: "fluentd.internal"
    port: 24224
```

## Azure AD App Registration

To collect Office365 audit logs, you need an Azure AD App Registration with these API permissions:

1. Go to Azure Portal → Azure Active Directory → App registrations
2. Create new registration
3. Add API permissions:
   - `Office 365 Management APIs`
     - `ActivityFeed.Read` (Application permission)
     - `ActivityFeed.ReadDlp` (Application permission) - for DLP.All
4. Grant admin consent
5. Create client secret
6. Note: `tenant_id`, `client_id`, `client_secret`

## Troubleshooting

### No logs collected
1. Check credentials are correct
2. Verify API permissions granted
3. Check `only_future_events` setting
4. Look at collector logs for API errors

### API errors
- `AF20055`: startTime/endTime invalid - check state files
- `401 Unauthorized`: Invalid credentials
- `403 Forbidden`: Missing API permissions

### State reset
To re-collect logs, delete state files:
```bash
rm -f office365-*.json
```
