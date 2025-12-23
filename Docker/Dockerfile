# Stage 1: Build the application
FROM rust:1.92-alpine3.23 AS builder

# Install build dependencies
RUN apk add --no-cache \
    musl-dev \
    pkgconfig \
    openssl-dev \
    openssl-libs-static \
    curl \
    git

# Set working directory
WORKDIR /app

# Copy the backend project files
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations

# Build the application in release mode
RUN cargo build --release

# Stage 2: Create minimal runtime image
FROM alpine:3.23

# Install runtime dependencies
RUN apk add --no-cache \
    ca-certificates \
    libgcc

# Create non-root user for security
RUN addgroup -S appgroup && adduser -S appuser -G appgroup

# Set working directory
WORKDIR /app

# Copy the compiled binary from builder stage
COPY --from=builder /app/target/release/option-chain-orderbook-backend /app/option-chain-orderbook-backend

# Change ownership to non-root user
RUN chown -R appuser:appgroup /app

# Switch to non-root user
USER appuser

# Expose the API port
EXPOSE 8080

# Set environment variables
ENV HOST=0.0.0.0
ENV PORT=8080
ENV RUST_LOG=info

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:8080/health || exit 1

# Run the application
CMD ["./option-chain-orderbook-backend"]
