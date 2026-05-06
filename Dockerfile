FROM rust:1.88-bookworm AS builder
WORKDIR /app

COPY . .

WORKDIR /app/apps/api
RUN cargo build --release -p verdict-api

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/apps/api/target/release/verdict-api /app/verdict-api

EXPOSE 3000
CMD ["/app/verdict-api"]
