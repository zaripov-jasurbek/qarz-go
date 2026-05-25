# ── Stage 1: сборка ──────────────────────────────────────────────────────────
FROM rust:1.85-slim AS builder
WORKDIR /app

# Сначала копируем только Cargo-файлы — зависимости кешируются отдельно.
# Если изменился только src/ — зависимости не перекомпилируются.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -f target/release/deps/loan_wallet*

# Теперь копируем реальный исходный код
COPY src ./src
RUN cargo build --release

# ── Stage 2: минимальный образ ───────────────────────────────────────────────
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/loan_wallet /usr/local/bin/loan_wallet

CMD ["loan_wallet"]
