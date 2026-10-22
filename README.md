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
| `crates/form-core/` | Rust field rules shared by the backend and web-wasm |
| `backend/` | Rust gRPC and gRPC-web server (tonic) |
| `web-react/` | React, TypeScript and Tailwind CSS frontend |
| `web-wasm/` | Rust canvas frontend compiled to WebAssembly |
| `e2e/` | Playwright tests of both frontends, including visual parity |
| `deploy/` | Kind cluster and Helm chart |

## Screenshots

Both frontends running on Kind, captured by `make screenshots`. The parity
test fails if more than 0.5% of the pixels differ between the two.

| State | React (nodes) | WebAssembly (pixels) |
| --- | --- | --- |
| Empty, desktop | <img src="docs/screenshots/react-empty-desktop.png" alt="React frontend, empty state, desktop" width="400"> | <img src="docs/screenshots/wasm-empty-desktop.png" alt="WebAssembly frontend, empty state, desktop" width="400"> |
| Empty, phone | <img src="docs/screenshots/react-empty-phone.png" alt="React frontend, empty state, phone" width="200"> | <img src="docs/screenshots/wasm-empty-phone.png" alt="WebAssembly frontend, empty state, phone" width="200"> |
| Errors, desktop | <img src="docs/screenshots/react-errors-desktop.png" alt="React frontend, errors state, desktop" width="400"> | <img src="docs/screenshots/wasm-errors-desktop.png" alt="WebAssembly frontend, errors state, desktop" width="400"> |
| Errors, phone | <img src="docs/screenshots/react-errors-phone.png" alt="React frontend, errors state, phone" width="200"> | <img src="docs/screenshots/wasm-errors-phone.png" alt="WebAssembly frontend, errors state, phone" width="200"> |
| Success, desktop | <img src="docs/screenshots/react-success-desktop.png" alt="React frontend, success state, desktop" width="400"> | <img src="docs/screenshots/wasm-success-desktop.png" alt="WebAssembly frontend, success state, desktop" width="400"> |
| Success, phone | <img src="docs/screenshots/react-success-phone.png" alt="React frontend, success state, phone" width="200"> | <img src="docs/screenshots/wasm-success-phone.png" alt="WebAssembly frontend, success state, phone" width="200"> |

## Running

Needs Rust with the `wasm32-unknown-unknown` target, wasm-pack, Node.js with
pnpm, Docker, Kind, kubectl and Helm.

| Command | What it does |
| --- | --- |
| `make build` | Builds the backend and both frontends |
| `make test` | Lints, type-checks and tests every project |
| `make deploy` | Builds the images and installs the chart on a `nodes-vs-pixels` Kind cluster |
| `make e2e` | Runs the Playwright tests against the cluster |
| `make screenshots` | Updates the screenshots above |
| `make lighthouse` | Audits both frontends, mobile and desktop |
| `make sonar` | Measures coverage and analyzes each project on SonarQube Cloud (needs `SONAR_TOKEN`) |
| `make kind-down` | Deletes the cluster |

Once deployed, the React frontend is at http://localhost:8081 and the
WebAssembly one at http://localhost:8082.
