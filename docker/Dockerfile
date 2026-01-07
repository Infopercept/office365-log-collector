# Multi-stage build for Office365 Audit Log Collector
# Stage 1: Build the Rust application
FROM rust:1.92.0-slim as builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy source code
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Build release binary
RUN cargo build --release

# Stage 2: Create minimal runtime image
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create app directory and state directory
RUN mkdir -p /app/state && \
    useradd -m -u 1001 office365 && \
    chown -R office365:office365 /app

WORKDIR /app

# Copy binary from builder
COPY --from=builder /build/target/release/office_audit_log_collector /usr/local/bin/

# Set correct permissions
RUN chmod +x /usr/local/bin/office_audit_log_collector

# Switch to non-root user
USER office365

# Health check (optional - checks if process is running)
HEALTHCHECK --interval=60s --timeout=10s --start-period=30s --retries=3 \
    CMD pgrep office_audit_log_collector || exit 1

# Default command - expects config at /app/config.yaml
ENTRYPOINT ["office_audit_log_collector"]
CMD ["--config", "/app/config.yaml"]
