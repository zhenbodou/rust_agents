# Rust 多阶段构建 + cargo-chef 依赖缓存（书 45.5）
FROM rust:1.88-slim AS chef
RUN cargo install cargo-chef --version 0.1.77 --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json
COPY . .
RUN cargo build --release --locked --bin eval-server --bin eval-healthcheck

FROM gcr.io/distroless/cc-debian12:nonroot AS runtime
COPY --from=builder /app/target/release/eval-server /usr/local/bin/eval-server
COPY --from=builder /app/target/release/eval-healthcheck /usr/local/bin/eval-healthcheck
COPY --from=builder /app/migrations /migrations
USER nonroot
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/eval-server"]
