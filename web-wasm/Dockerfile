# Build context: repository root (the app needs the shared proto, design, spec and seo sources).
FROM rust:1.99-slim-trixie AS build
WORKDIR /src
RUN apt-get update \
 && apt-get install -y --no-install-recommends make nodejs \
 && rm -rf /var/lib/apt/lists/*
RUN rustup target add wasm32-unknown-unknown \
 && cargo install --locked wasm-pack@0.15.0
COPY Cargo.toml Cargo.lock ./
COPY proto proto
COPY design design
COPY spec spec
COPY seo seo
COPY crates crates
COPY backend backend
COPY web-wasm web-wasm
# Public URL used for canonical and Open Graph tags; defaults to the Kind deployment.
ARG SITE_URL=""
RUN SITE_URL="$SITE_URL" make -C web-wasm build

FROM nginxinc/nginx-unprivileged:1.29-alpine
ENV BACKEND_URL=http://backend:50051
COPY deploy/nginx/default.conf.template /etc/nginx/templates/default.conf.template
COPY --from=build /src/web-wasm/dist /usr/share/nginx/html
EXPOSE 8080
