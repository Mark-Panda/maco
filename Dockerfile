# 构建阶段：adk-rust 1.0.0 要求 rustc >= 1.94
FROM rust:1.94-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY migrations ./migrations
RUN cargo build --release -p maco-server --no-default-features \
    && strip /app/target/release/maco-server

# 运行阶段
FROM debian:bookworm-slim AS runtime

# 国内或 deb.debian.org 不稳定时可传：--build-arg APT_MIRROR=mirrors.aliyun.com
ARG APT_MIRROR=
RUN set -eux; \
    if [ -n "${APT_MIRROR}" ]; then \
      for f in /etc/apt/sources.list /etc/apt/sources.list.d/debian.sources; do \
        [ -f "$f" ] && sed -i "s|deb.debian.org|${APT_MIRROR}|g; s|security.debian.org|${APT_MIRROR}|g" "$f" || true; \
      done; \
    fi; \
    for i in 1 2 3 4 5; do \
      apt-get update && break; \
      echo "apt-get update failed (attempt ${i}), retry in 5s..."; \
      sleep 5; \
    done; \
    apt-get install -y --no-install-recommends ca-certificates curl bash gosu; \
    rm -rf /var/lib/apt/lists/*

RUN useradd -m -u 1000 -s /bin/bash maco
COPY --from=builder /app/target/release/maco-server /usr/local/bin/maco-server
COPY docker/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh
WORKDIR /home/maco
ENV HOME=/home/maco
EXPOSE 8080
ENTRYPOINT ["/entrypoint.sh"]
