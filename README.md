# Nodes vs Pixels

One sign-up form, built twice:

- **web-react**: React, TypeScript and Tailwind CSS render DOM nodes.
- **web-wasm**: Rust compiled to WebAssembly paints pixels on a `<canvas>`.

Both frontends talk to the same Rust **backend** over gRPC-web.

## Layout

| Path | Purpose |
| --- | --- |
| `proto/` | gRPC API contract |
| `design/` | Design tokens and font shared by both frontends |
| `spec/` | UI copy and validation conformance cases shared by every project |
| `seo/` | Single source of SEO metadata, injected into both frontends |
