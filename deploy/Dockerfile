# syntax=docker/dockerfile:1
FROM rust:1.94-bookworm AS rust-build
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --locked --release -p starter-api -p starter-mcp-server \
 && mkdir -p /out \
 && cp target/release/starter-api target/release/starter-mcp-server /out/

FROM node:22-bookworm AS web-build
COPY --from=oven/bun:1.4.2-debian /usr/local/bin/bun /usr/local/bin/bun
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/root/.bun/install/cache bun install --frozen-lockfile
ARG APP_URL
ARG LANDING_URL
ARG DOCS_URL
ARG API_PUBLIC_URL
ENV APP_URL=${APP_URL}
ENV LANDING_URL=${LANDING_URL}
ENV DOCS_URL=${DOCS_URL}
ENV API_PUBLIC_URL=${API_PUBLIC_URL}
ENV API_INTERNAL_URL=http://127.0.0.1:4000
ENV NEXT_TELEMETRY_DISABLED=1
RUN touch .env \
 && (cd apps/app && node node_modules/next/dist/bin/next build) \
 && (cd apps/docs && node node_modules/next/dist/bin/next build) \
 && (cd apps/landing && node node_modules/next/dist/bin/next build) \
 && rm .env

FROM node:22-bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates libssl3 \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /srv
COPY --from=web-build /src /srv
COPY --from=rust-build /out/starter-api /usr/local/bin/starter-api
COPY --from=rust-build /out/starter-mcp-server /usr/local/bin/starter-mcp-server
