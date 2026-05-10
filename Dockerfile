FROM rust:trixie AS builder
WORKDIR /usr/src/itsf-playerdb
RUN apt-get update && \
    apt-get install -y --no-install-recommends cmake && \
    rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY migrations ./migrations
COPY src ./src
RUN cargo build --release

FROM debian:trixie-slim
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 && \
    rm -rf /var/lib/apt/lists/*

RUN useradd -m -u 10001 app
WORKDIR /app
COPY --from=builder /usr/src/itsf-playerdb/target/release/server /app/itsf-playerdb
COPY html /app/html
RUN mkdir -p /app/db && chown -R app:app /app

USER app
ENV DATABASE_URL=/app/db/players.sqlite
ENV HTML_ROOT=/app/html
ENV PORT=8080
VOLUME ["/app/db"]
EXPOSE 8080

CMD ["/app/itsf-playerdb"]
