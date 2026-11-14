FROM rust:1.96.0-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM gcr.io/distroless/cc-debian12

EXPOSE 8080

COPY --from=builder /app/target/release/fatsecret-mcp-server /usr/local/bin/fatsecret-mcp-server

ENTRYPOINT ["fatsecret-mcp-server"]
