# Build context: repository root (the app needs the shared proto, design, spec and seo sources).
FROM node:24-alpine AS build
WORKDIR /src
RUN corepack enable
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY seo/package.json seo/
COPY web-react/package.json web-react/
RUN pnpm install --frozen-lockfile --filter web-react...
COPY proto proto
COPY design design
COPY spec spec
COPY seo seo
COPY web-react web-react
# Public URL used for canonical and Open Graph tags; defaults to the Kind deployment.
ARG SITE_URL=""
RUN SITE_URL="$SITE_URL" pnpm --filter web-react build

FROM nginxinc/nginx-unprivileged:1.29-alpine
ENV BACKEND_URL=http://backend:50051
COPY deploy/nginx/default.conf.template /etc/nginx/templates/default.conf.template
COPY --from=build /src/web-react/dist/client /usr/share/nginx/html
EXPOSE 8080
