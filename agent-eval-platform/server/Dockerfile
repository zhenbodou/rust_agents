# Rust 多阶段构建 + cargo-chef 依赖缓存（书 45.5）
FROM rust:1.85-slim AS chef
RUN cargo install cargo-chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release --bin eval-server

FROM gcr.io/distroless/cc-debian12:nonroot AS runtime
COPY --from=builder /app/target/release/eval-server /usr/local/bin/eval-server
COPY --from=builder /app/migrations /migrations
USER nonroot
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/eval-server"]
