# Build context: repository root (the backend needs the shared proto and form-core crate).
FROM rust:1.99-slim-trixie AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY proto proto
COPY crates crates
COPY backend backend
RUN cargo build --release --locked -p backend

FROM gcr.io/distroless/cc-debian13:nonroot
COPY --from=build /src/target/release/backend /usr/local/bin/backend
USER nonroot
EXPOSE 50051
ENTRYPOINT ["/usr/local/bin/backend"]
