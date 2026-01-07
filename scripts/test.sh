#!/bin/bash
# Test Script - Office365 Collector
# Phase 1: Test WITHOUT Fluentd

echo "=========================================="
echo "Office365 Collector - Test Script"
echo "Phase 1: Testing WITHOUT Fluentd"
echo "=========================================="
echo ""

# Check if config exists
if [ ! -f config/config-test.yaml ]; then
    echo "❌ ERROR: config/config-test.yaml not found"
    echo "Please create it from config/config.yaml.template"
    exit 1
fi

# Check if credentials are filled in
if grep -q "YOUR-TENANT-ID-HERE" config/config-test.yaml; then
    echo "⚠️  WARNING: Credentials not filled in!"
    echo "Please edit config/config-test.yaml and add your credentials"
    echo ""
    echo "You need:"
    echo "  - tenant_id (from Azure AD)"
    echo "  - client_id (from App Registration)"
    echo "  - client_secret (from Client Secret)"
    echo ""
    echo "See docs/CREDENTIALS-CHECKLIST.md for details"
    exit 1
fi

echo "✅ Config file found"
echo "✅ Credentials appear to be filled in"
echo ""

# Check if binary exists
if [ ! -f target/release/office_audit_log_collector ]; then
    echo "❌ ERROR: Binary not found"
    echo "Run: cargo build --release"
    exit 1
fi

echo "✅ Binary found"
echo ""

# Create state directory
mkdir -p state

echo "🚀 Starting Office365 Collector..."
echo "=========================================="
echo ""

# Run the collector
./target/release/office_audit_log_collector --config config/config-test.yaml

echo ""
echo "=========================================="
echo "Test Complete!"
echo ""

# Check if state files were created
if ls state/office365-*.json 1> /dev/null 2>&1; then
    echo "✅ State files created:"
    ls -lh state/office365-*.json
    echo ""
    echo "📄 State file content:"
    cat state/office365-*.json | head -20
else
    echo "⚠️  No state files found (this might be OK for first run)"
fi

echo ""
echo "=========================================="
echo "Next Steps:"
echo "1. Check output above for errors"
echo "2. Look for 'Successfully logged in' message"
echo "3. If successful, proceed to Phase 2 (with Fluentd)"
echo "=========================================="
