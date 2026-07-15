# --- Frontend ---
FROM node:22-bookworm-slim AS frontend
RUN corepack enable
WORKDIR /frontend
COPY frontend/package.json frontend/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY frontend/ ./
RUN pnpm build

# --- Backend ---
FROM rust:1.96-bookworm AS backend
WORKDIR /app
COPY backend/Cargo.toml backend/Cargo.lock ./
COPY backend/migrations ./migrations
COPY backend/src ./src
RUN mkdir -p static \
  && cargo build --release \
  && strip target/release/audiobooker

# --- Runtime ---
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=backend /app/target/release/audiobooker /app/audiobooker
COPY --from=frontend /frontend/dist /app/static

RUN mkdir -p /data

ENV HOST=0.0.0.0
ENV PORT=3000
ENV DATABASE_PATH=/data/audiobooker.db
ENV STATIC_DIR=/app/static
ENV COOKIE_SECURE=false

EXPOSE 3000
CMD ["/app/audiobooker"]
