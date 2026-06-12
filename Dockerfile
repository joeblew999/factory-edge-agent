# Build the edge-agent binary (fetches factory-machine-model + factory-howick-driver from GitHub).
FROM rust:1.95-slim AS build
RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config libssl-dev git ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY . .
RUN cargo build --release --bin factory-edge-agent

# Slim runtime image.
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
        && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/factory-edge-agent /usr/local/bin/factory-edge-agent
ENTRYPOINT ["factory-edge-agent"]
CMD ["--config", "/config/agent.toml"]
