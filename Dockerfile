# Sori — 무손실 음악 서버 (러스트)
#
#   docker build -t sori .
#   docker run -p 4380:4380 -v /내/음악:/music:ro -v sori-data:/data sori

FROM node:22-bookworm-slim AS ui
WORKDIR /ui
COPY ui/package.json ui/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY ui/ ./
RUN npx vite build

FROM rust:1-bookworm AS server
WORKDIR /src
COPY Cargo.toml Cargo.lock rustfmt.toml ./
COPY crates ./crates
# 화면은 실행 파일 안에 들어간다.
COPY --from=ui /ui/dist ./ui/dist
RUN cargo build --release -p sori-server

FROM debian:bookworm-slim
RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates \
  && rm -rf /var/lib/apt/lists/* \
  && useradd --system --uid 10001 sori \
  && mkdir -p /data /music \
  && chown sori:sori /data
COPY --from=server /src/target/release/sori /usr/local/bin/sori
ENV SORI_MUSIC_DIR=/music \
    SORI_DATA_DIR=/data \
    SORI_HOST=0.0.0.0 \
    SORI_PORT=4380
USER sori
VOLUME ["/data"]
EXPOSE 4380
CMD ["sori"]
