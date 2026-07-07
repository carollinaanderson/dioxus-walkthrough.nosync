# Build stage: dioxus-cli builds the wasm client + server binary in one bundle.
FROM rust:1 AS builder
RUN cargo install cargo-binstall --locked \
    && cargo binstall dioxus-cli --no-confirm
WORKDIR /app
COPY . .
RUN dx bundle --platform web --release

# Runtime stage: just the server binary + static assets.
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/dx/ch07-background-jobs/release/web/ /usr/local/app/
ENV PORT=8080
ENV IP=0.0.0.0
EXPOSE 8080
WORKDIR /usr/local/app
CMD ["/usr/local/app/server"]
