# Sori — 셀프호스팅 무손실 스트리밍
#
#   docker build -t sori .
#   docker run -p 3000:3000 -v /내/음악:/music:ro -v sori-data:/data sori

FROM node:22-bookworm-slim AS build
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY . .
ENV NEXT_TELEMETRY_DISABLED=1 SORI_STANDALONE=1
RUN npm run build

FROM node:22-bookworm-slim
# ffmpeg: 브라우저가 못 여는 형식(ALAC, DSD, AIFF, WavPack, APE)을 FLAC으로 바꿔 보낸다.
RUN apt-get update \
  && apt-get install -y --no-install-recommends ffmpeg ca-certificates \
  && rm -rf /var/lib/apt/lists/* \
  && mkdir -p /data /music \
  && chown node:node /data
WORKDIR /app
ENV NODE_ENV=production \
    NEXT_TELEMETRY_DISABLED=1 \
    PORT=3000 \
    HOSTNAME=0.0.0.0 \
    MUSIC_DIR=/music \
    DATA_DIR=/data
COPY --from=build --chown=node:node /app/.next/standalone ./
COPY --from=build --chown=node:node /app/.next/static ./.next/static
USER node
VOLUME ["/data"]
EXPOSE 3000
CMD ["node", "server.js"]
