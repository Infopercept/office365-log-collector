#!/bin/bash
set -e

echo "=================================="
echo "Office365 Collector - Production Deployment"
echo "=================================="
echo ""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Paths
COLLECTOR_DIR="/home/ubuntu/new-siem/office365-audit-log-collector"
SIEM_DIR="/home/ubuntu/new-siem/ocsf-siem-deployment"
LOGS_DIR="/var/logs/office365"

echo "Step 1: Checking prerequisites..."

# Check if config.production.yaml exists
if [ ! -f "$COLLECTOR_DIR/config/config.production.yaml" ]; then
    echo -e "${RED}ERROR: config/config.production.yaml not found!${NC}"
    echo "Please create it from the template:"
    echo "  cp config/config.production.template.yaml config/config.production.yaml"
    echo "  nano config/config.production.yaml  # Fill in your credentials"
    exit 1
fi

echo -e "${GREEN}✓ Configuration file found${NC}"

# Check if Rust is installed
if ! command -v cargo &> /dev/null; then
    echo -e "${YELLOW}⚠ Rust not found. Installing...${NC}"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

echo -e "${GREEN}✓ Rust toolchain ready${NC}"

# Check if Docker is running
if ! docker info &> /dev/null; then
    echo -e "${RED}ERROR: Docker is not running!${NC}"
    echo "Please start Docker first."
    exit 1
fi

echo -e "${GREEN}✓ Docker is running${NC}"
echo ""

echo "Step 2: Building Rust binary..."
cd "$COLLECTOR_DIR"
~/.cargo/bin/cargo build --release
echo -e "${GREEN}✓ Rust binary built${NC}"
echo ""

echo "Step 3: Building Docker image..."
docker build -f docker/Dockerfile -t office365-audit-log-collector:latest .
echo -e "${GREEN}✓ Docker image built${NC}"
echo ""

echo "Step 4: Creating log directories..."
sudo mkdir -p "$LOGS_DIR/archives"
sudo chown -R $USER:$USER "$LOGS_DIR"
echo -e "${GREEN}✓ Log directories created${NC}"
echo ""

echo "Step 5: Verifying Vector configuration..."
if [ ! -d "$SIEM_DIR/vector/datasources/office365" ]; then
    echo -e "${YELLOW}⚠ Vector Office365 config not found. Please add to docker-compose.yml${NC}"
    echo "See: $COLLECTOR_DIR/docker/docker-compose.snippet.yml"
else
    echo -e "${GREEN}✓ Vector configuration exists${NC}"
fi
echo ""

echo "Step 6: Deploying to Docker..."
cd "$SIEM_DIR"

# Check if office365-collector service exists in docker-compose.yml
if ! grep -q "office365-collector:" docker-compose.yml; then
    echo -e "${YELLOW}⚠ office365-collector not in docker-compose.yml${NC}"
    echo "Please add the service from: $COLLECTOR_DIR/docker/docker-compose.snippet.yml"
    echo ""
    echo "Would you like to view the snippet now? (y/n)"
    read -r response
    if [[ "$response" =~ ^[Yy]$ ]]; then
        cat "$COLLECTOR_DIR/docker/docker-compose.snippet.yml"
    fi
    exit 1
fi

echo "Starting office365-collector..."
docker-compose up -d office365-collector

echo -e "${GREEN}✓ Office365 collector started${NC}"
echo ""

echo "Step 7: Restarting Vector to load new configs..."
docker-compose restart vector
echo -e "${GREEN}✓ Vector restarted${NC}"
echo ""

echo "=================================="
echo -e "${GREEN}Deployment Complete!${NC}"
echo "=================================="
echo ""
echo "Monitor logs with:"
echo "  docker logs -f office365-collector"
echo "  tail -f $LOGS_DIR/collector.log"
echo "  tail -f $LOGS_DIR/office365.json"
echo ""
echo "Check Kafka events:"
echo "  docker exec kafka kafka-console-consumer \\"
echo "    --bootstrap-server localhost:9092 \\"
echo "    --topic ocsf.events \\"
echo "    --from-beginning \\"
echo "    --max-messages 10 | jq ."
echo ""
echo "View all logs:"
echo "  docker-compose logs -f office365-collector vector kafka"
echo ""
