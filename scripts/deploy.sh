#!/bin/bash
# One-Shot Deployment Script for Office365 Audit Log Collector
# Usage: ./deploy.sh

set -e

echo "=========================================="
echo "Office365 Audit Log Collector Deployment"
echo "=========================================="
echo ""

# Check if Docker is installed
if ! command -v docker &> /dev/null; then
    echo "❌ ERROR: Docker is not installed"
    echo "Install Docker: https://docs.docker.com/engine/install/"
    exit 1
fi

# Check if Docker Compose is installed
if ! command -v docker-compose &> /dev/null; then
    echo "❌ ERROR: Docker Compose is not installed"
    echo "Install Docker Compose: https://docs.docker.com/compose/install/"
    exit 1
fi

echo "✅ Docker installed: $(docker --version)"
echo "✅ Docker Compose installed: $(docker-compose --version)"
echo ""

# Check if config.yaml exists
if [ ! -f config/config.yaml ]; then
    echo "⚠️  Config file not found. Creating from template..."
    if [ -f config/config.production.yaml ]; then
        cp config/config.production.yaml config/config.yaml
        echo "✅ Created config/config.yaml from template"
        echo ""
        echo "⚠️  IMPORTANT: You must edit config/config.yaml with your Office365 credentials!"
        echo ""
        read -p "Do you want to edit config/config.yaml now? (y/n) " -n 1 -r
        echo ""
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            ${EDITOR:-nano} config/config.yaml
        else
            echo "Please edit config/config.yaml before running this script again."
            echo "See docs/CREDENTIALS-CHECKLIST.md for setup instructions."
            exit 1
        fi
    else
        echo "❌ ERROR: config/config.production.yaml template not found"
        exit 1
    fi
fi

# Validate config.yaml has credentials filled in
if grep -q "YOUR-TENANT-ID-HERE" config/config.yaml; then
    echo "❌ ERROR: config/config.yaml still contains placeholder values"
    echo "Please edit config/config.yaml and add your Office365 credentials"
    echo ""
    echo "Required fields:"
    echo "  - tenant_id"
    echo "  - client_id"
    echo "  - client_secret"
    echo ""
    echo "See docs/CREDENTIALS-CHECKLIST.md for details"
    exit 1
fi

echo "✅ Config file found and validated"
echo ""

# Check if fluentd config exists
if [ ! -f fluentd/fluent.conf ]; then
    echo "❌ ERROR: fluentd/fluent.conf not found"
    exit 1
fi

echo "✅ Fluentd config found"
echo ""

# Build and start services
echo "🚀 Building Docker images..."
docker-compose -f docker/docker-compose.production.yaml build

echo ""
echo "🚀 Starting services..."
docker-compose -f docker/docker-compose.production.yaml up -d

echo ""
echo "✅ Deployment complete!"
echo ""
echo "=========================================="
echo "Service Status:"
echo "=========================================="
docker-compose -f docker/docker-compose.production.yaml ps
echo ""

echo "=========================================="
echo "Next Steps:"
echo "=========================================="
echo "1. View logs:    docker-compose -f docker/docker-compose.production.yaml logs -f"
echo "2. Check status: docker-compose -f docker/docker-compose.production.yaml ps"
echo "3. Stop:         docker-compose -f docker/docker-compose.production.yaml down"
echo ""
echo "Wait 1-2 minutes, then verify logs are being collected:"
echo "  docker exec office365-collector ls -lh /app/state/"
echo ""
echo "See docs/DOCKER-DEPLOYMENT.md for full documentation."
echo "=========================================="
