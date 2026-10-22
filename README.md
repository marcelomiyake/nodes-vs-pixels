# Nodes vs Pixels

One sign-up form, built twice:

- **web-react**: React, TypeScript and Tailwind CSS render DOM nodes.
- **web-wasm**: Rust compiled to WebAssembly paints every pixel on a
  single `<canvas>`.

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

## The canvas frontend

The WebAssembly page has no inputs, labels or button: it is one focusable,
labelled `<canvas>` in the page's `<main>` element. Rust lays the form out
with the same design tokens and font as the React app, paints it, and
handles everything Chrome's native controls do for the React app:

- focus: Tab and Shift+Tab through the controls and out of the page, also
  coming in from the browser's own controls, with the focus ring shown after
  keyboard use and the focused control scrolled into view;
- typing: caret movement and deletion by character, word and line with
  Chrome's shortcuts on Linux and macOS, selection with Shift, undo and
  redo across the page's inputs, the caret placed in mixed left-to-right
  and right-to-left text, and grapheme clusters and emoji sequences kept
  whole;
- the mouse: clicks, double and triple clicks, dragging a selection (with
  autoscroll past an input's sides), labels that focus their input, the
  wheel scrolling an input sideways, and the cursor and hover each element
  shows;
- the clipboard: copy, cut and paste, never copying a password;
- scrolling: the page follows the caret while typing, and keeps what is in
  view in place as messages above it come and go, as Chrome's scroll
  anchoring does.

Drawing every pixel has a price. Compared with the React app, the canvas
page has:

- no input method, so no composed input for Chinese, Japanese or Korean,
  and no on-screen keyboard on phones;
- no autofill or password manager, and no spell checking;
- no form for screen readers, which see only the labelled canvas;
- no context menu, and no selecting the page's text;
- Linux and macOS editing shortcuts only, without Windows' word movement
  or macOS's Emacs-style Control keys;
- carets a few pixels off Chrome's in some scripts, such as Burmese,
  Sinhala, Kannada and Khmer, whose shaping the canvas measures differently.

## How it is built

[Claude Code](https://claude.com/claude-code) (Claude Opus 5.5) writes the
code, and [Codex](https://developers.openai.com/codex) (GPT-6.1 Sol, medium
reasoning effort) reviews every commit before it reaches `main`:

1. Claude Code makes each change on a short-lived branch as a
   [Conventional Commit](https://www.conventionalcommits.org) of at most
   1,000 changed lines, lockfiles aside, with every project's lints and
   tests passing.
2. Codex reviews each commit locally:

   ```sh
   codex review --commit <sha> -c model='"gpt-6.1-sol"' -c model_reasoning_effort='"medium"'
   ```

3. Claude Code fixes each finding in the commit it concerns, or, when
   Chrome's behavior shows the finding is wrong, keeps the code and notes
   why. Codex then reviews the changed commits again, until none has a
   finding left.
4. `main` fast-forwards to the branch, so it has no merge commits and
   every commit on it was reviewed.

The repository has no pull requests or CI: the reviews run on the
developer's machine. To have Codex review pull requests on GitHub instead,
there are two ways:

- **The Codex GitHub app.** Connect the repository to Codex, enable code
  review for it in the Codex settings, and turn on *Automatic review* under
  *Personal preferences*; Codex then reviews pull requests as the *Review
  trigger* setting says. A comment that mentions `@codex review` asks for a
  review at any time, and a `## Code Review Rules` section in `AGENTS.md`
  tells Codex what to look for.
- **A GitHub Action.** A workflow such as
  `.github/workflows/codex-review.yml` runs
  [`openai/codex-action`](https://github.com/openai/codex-action) on each
  pull request, with an `OPENAI_API_KEY` repository secret, and posts its
  review as a comment. GitHub gives no secrets and only a read-only token to
  pull requests from forks, so it reviews only those from the repository's
  own branches:

  ```yaml
  on:
    pull_request:
      types: [opened, synchronize]
  jobs:
    review:
      if: github.event.pull_request.head.repo.full_name == github.repository
      runs-on: ubuntu-latest
      permissions:
        contents: read
      outputs:
        review: ${{ steps.codex.outputs.final-message }}
      steps:
        - uses: actions/checkout@v5
          with:
            ref: refs/pull/${{ github.event.pull_request.number }}/merge
            fetch-depth: 0
            persist-credentials: false
        - id: codex
          uses: openai/codex-action@v1
          with:
            openai-api-key: ${{ secrets.OPENAI_API_KEY }}
            model: gpt-6.1-sol
            effort: medium
            permission-profile: ":workspace"
            prompt: Review the changes of pull request #${{ github.event.pull_request.number }} against its base branch and report bugs.
    comment:
      needs: review
      if: needs.review.outputs.review != ''
      runs-on: ubuntu-latest
      permissions:
        issues: write
        pull-requests: write
      steps:
        - uses: actions/github-script@v7
          env:
            REVIEW: ${{ needs.review.outputs.review }}
          with:
            script: |
              await github.rest.issues.createComment({
                ...context.repo,
                issue_number: context.payload.pull_request.number,
                body: process.env.REVIEW,
              });
  ```

## What each frontend cost to build

Claude Code built the repository in one session, from `chore: scaffold
monorepo workspace` to `fix(web-wasm): put the canvas in a main element`. It ran Claude Opus 5.5
at extra-high effort for 11 hours, 7.3 of them active, in 1,148 API calls
with 9 automatic compactions of its context. Claude Code estimated the
session's cost at $110.52.

Its log splits that work by area. Each API call goes to the area that its
tool calls' paths and commands name (`web-react/` or `pnpm --filter
web-react`; `web-wasm/`, `wasm-pack` or `wasm32`; both, as in the
Playwright tests; or the shared projects), and a call without tool calls
goes to the area of the call before it:

| Area | API calls | Output tokens | All tokens | Active time |
| --- | --- | --- | --- | --- |
| `web-react` | 94 | 0.14 M | 22 M | 0.6 h |
| `web-wasm` | 663 | 0.98 M | 150 M | 3.7 h |
| Both frontends: e2e and parity tests, builds of both | 266 | 0.44 M | 59 M | 2.3 h |
| Shared: contract, tokens, `form-core`, backend, SEO, deploy | 125 | 0.12 M | 22 M | 0.7 h |
| Total | 1,148 | 1.67 M | 254 M | 7.3 h |

Output tokens include the model's thinking. All tokens adds the input
each call reads, which is almost all of them: every call reads the whole
context again, about 220,000 tokens on average, mostly from the prompt
cache. Active time counts the time from each call to the next, including
the tools it ran, up to 5 minutes.

The canvas frontend took about 7 times the React one: 7 times the API
calls and output tokens, 6.8 times the tokens and 6 times the active time.
Counting the work on both frontends half to each, it took 3.5 times as
much; counting all of it to the canvas frontend, whose parity with the
React one those tests check, 9.5 times. Every measure shares the session
alike, so the cost does too: about $65 for `web-wasm`, $10 for
`web-react`, $26 for both and $10 for the shared projects.

The canvas frontend was built twice. Its first version painted the form
but laid Chrome's native inputs over the canvas to take the keys, mouse
and clipboard. Until then, the canvas frontend had taken 50 M tokens
against React's 18 M, 2.8 times as many. Once the native inputs went,
the canvas had to do everything they did, from carets and selection to
undo, scrolling and the clipboard. That rework took the other 101 M of
its tokens, and 32 M more on work on both frontends, mostly the tests
that compare them.

Codex's reviews show the same gap. It ran `codex review` 104 times,
with GPT-6.1 Sol at medium effort, each in a session whose log names the
commit it reviewed. A commit was reviewed again after each fix, so a
commit's reviews are its review rounds:

| Area | Reviews | Commits | Input tokens | Output tokens | Review time |
| --- | --- | --- | --- | --- | --- |
| `web-react` | 13 | 6 | 2.3 M | 11 k | 0.2 h |
| `web-wasm` | 73 | 23 | 26.2 M | 157 k | 1.9 h |
| Shared, tests and docs | 18 | 15 | 2.1 M | 12 k | 0.2 h |
| Total | 104 | 44 | 30.6 M | 180 k | 2.3 h |

Output tokens include the model's reasoning, and 88% of the input came
from Codex's cache. Review time adds up each review's run, though some
ran in parallel.

- **The canvas frontend took 11 times React's review tokens** and 12 times
  its review time. It had more commits, each took more rounds, 3.2 on
  average against 2.2 and up to 9 for `feat(web-wasm): break canvas text
  where Chrome does`, and each review read twice as much, 358,000 tokens
  against 178,000.
- **Reviews took an eighth as many tokens as writing the code:** 31 M
  against Claude Code's 254 M. With both counted, the canvas frontend took
  177 M tokens and the React one 24 M, 7.3 times as many.
- **Codex ran on a ChatGPT Plus plan,** so its reviews had no per-token
  cost. Over the session, the account's use of its weekly Codex limit rose
  from 1% to 26%.

Every commit made in the session has a review except
`refactor(web-wasm): remove the overlay of native controls`, which no
review log names.

The code shows the same gap:

| | `web-react` | `web-wasm` |
| --- | --- | --- |
| Commits | 6 | 24 |
| Lines of source, with tests | 1,076 (373 of tests) | 7,553 (about 2,665 of tests) |

These numbers leave out Claude Code's context compactions (about 2% of
its session's tokens) and the later sessions that wrote documentation
only.

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

## Inspecting CPU and memory

Chrome can show the CPU and memory use of both frontends, the canvas one
in less detail. Neither app measures itself: the code has no performance
marks or memory counters, and `make lighthouse` audits page loads only.
Its JSON reports in `lighthouse/` give the load's CPU time in the
`bootup-time` and `mainthread-work-breakdown` audits, and nothing on
memory.

Measure one app at a time, against the Kind deployment (`make deploy`),
in a Guest or Incognito window so that extensions add no work of their
own. Both apps are on `localhost`, so Chrome may run their tabs in one
process; close the other app's tabs first.

### Chrome's tools

| Tool | How to open it | What it shows | What it does not show |
| --- | --- | --- | --- |
| Task Manager | Shift+Esc on Linux and Windows, *Window > Task Manager* on macOS | The tab's CPU share and memory footprint, and its JavaScript and GPU memory once those columns are added with a right-click | Which code or object uses them. Chrome rasterizes pages in its GPU process, a separate row shared by every tab, so part of either app's drawing cost is not in the tab's row |
| Performance monitor | DevTools, *⋮ > More tools > Performance monitor* | Live CPU use, JS heap size, DOM nodes, JS event listeners, and layouts and style recalculations per second | Which function runs. The JS heap size leaves out WebAssembly memory and canvas pixels |
| Performance panel | DevTools, *Performance*, then record while using the page | Every main-thread task, split into scripting, rendering and painting, with a flame chart of the functions that ran; *Bottom-up* grouped by URL splits the time by file | Readable function names in the deployed builds (see below) |
| Memory panel | DevTools, *Memory* | Heap snapshots of the JavaScript heap, to compare before and after an action, and allocation sampling | What is inside WebAssembly memory |
| `performance.memory` | DevTools console | The JS heap's used and total size | Exact values: Chrome rounds them (both apps report 10,000,000 bytes) unless it is started with `--enable-precise-memory-info` |

`performance.measureUserAgentSpecificMemory()`, which counts all of a
page's memory, is not available on either app: it needs a cross-origin
isolated page, and `deploy/nginx/default.conf.template` sends no
`Cross-Origin-Opener-Policy` or `Cross-Origin-Embedder-Policy` header.

To measure CPU while you use a page:

1. Open the app, then DevTools (F12), and choose *Performance*.
2. Under the panel's settings, set *CPU throttling* to a 4× slowdown, as
   Lighthouse does for phones, so that short tasks stand out.
3. Click *Record*, use the page (type in a field, Tab through the form,
   submit it empty, then fill it in), and click *Stop*.
4. Select a stretch of the *Main* track: *Summary* splits its time into
   scripting, rendering and painting, and *Bottom-up* lists the functions
   that took the most.

To measure memory:

1. Open *Memory*, choose *Heap snapshot*, and take one after the page loads.
2. Use the page, click the trash-can icon to collect garbage, and take a
   second snapshot.
3. Show the second one in the *Comparison* view to list what was added and
   what stays alive.

### React

Each tool above works on the React app, but the deployed bundle is
minified and has no source maps (`web-react/vite.config.ts` does not set
`build.sourcemap`), so its functions show as `Nn` or `Bt` and the heap
snapshots' constructors are minified too (React's fibers show as `Li`).
DOM nodes and React's state, closures and listeners all appear in heap
snapshots, so they cover the app's own memory.

For readable names, profile the development server:

```sh
pnpm --filter web-react dev
```

It serves http://localhost:5173 with React's development build, whose
functions keep their names and which the
[React Developer Tools](https://react.dev/learn/react-developer-tools)
*Profiler* can time component by component; on the deployed production
build the extension shows the component tree only. The development build
runs extra checks, and `<StrictMode>` renders each component twice, so use
it to find where time goes, not to compare timings with the canvas app.
Submitting the form needs the backend at `127.0.0.1:50051`
(`cargo run -p backend`), where the dev server proxies gRPC-web.

### WebAssembly

The tools work on the canvas app too, with less detail:

- **CPU.** The Rust code shows in the Performance panel as
  `wasm-function[209]` and the like: `wasm-pack build --release` runs
  wasm-opt, which removes function names. The JavaScript bindings keep
  theirs, such as `__wbg_set_font_…` and `__wbg_set_fillStyle_…`, which
  shows which canvas calls take the time. To see the Rust names, build the
  module without wasm-opt and serve the page yourself:

  ```sh
  make -C web-wasm build
  cd web-wasm
  wasm-pack build --target web --release --no-opt --no-typescript --no-pack --out-dir dist/pkg
  python3 -m http.server -d dist 8090
  ```

  At http://localhost:8090 the frames then read, for example,
  `web_wasm::text::break_opportunities`. This module is almost twice the
  size of the deployed one and is not optimized by wasm-opt, so use it to
  find where time goes, not for timings. Submitting fails there, as the
  Python server does not proxy gRPC-web; typing, focus and painting work.
  `make -C web-wasm build` puts the optimized module back.
- **Memory.** Rust allocates in the module's linear memory, which Chrome
  sees as one `ArrayBuffer`. A heap snapshot lists it as
  `system / JSArrayBufferData` (about 1.2 MB once the form is painted),
  and the compiled code as `(WasmNativeModule)`, but not the Rust values
  inside, and allocation sampling records no Rust allocation. Linear
  memory grows and never shrinks, so its size is the most the app has
  needed. Its size now, from the console:

  ```js
  (await (await import('/pkg/web_wasm.js')).default()).memory.buffer.byteLength
  ```

  The bindings' `init` returns the running module when called again, so
  this starts nothing new.
- **The canvas.** Its pixel buffer covers the whole page, not just the
  viewport, at the screen's resolution: page width × page height × the
  device pixel ratio squared × 4 bytes, 16.7 MiB for a 1280 × 856 page
  on a 2× screen. It is not in the JavaScript heap. With GPU raster,
  Chrome keeps it, and copies of it, in GPU memory, which the Task
  Manager's *GPU memory* column shows, not the memory footprint. With
  software raster, it is in the tab's memory footprint.
- **Idle CPU.** While an input is focused, the app repaints the canvas
  every 500 ms to blink the caret (`BLINK_MS` in `web-wasm/src/web.rs`),
  so the Performance monitor shows some CPU use while the page sits idle.
  The React app leaves its caret to Chrome and runs no script then.

### Measured

Both apps were driven through the same steps in headless Chrome 156,
against the Kind deployment, on an Intel Core i7-8750H with Linux and an
NVIDIA GeForce GTX 1060, at 1280 × 800. Chrome was started with
`--enable-gpu` for GPU raster and without it for software raster. Each
run used a fresh browser. The CPU and memory tables give the median of
5 runs per app, and the GPU memory table the range of 2 or 3 further
runs. The steps were:

1. Load the page, and wait 3 s after it is painted.
2. Wait 10 s.
3. Press Tab to focus the first field, and wait 10 s.
4. Press Enter to submit the empty form, and wait 1 s.
5. Type the four fields, 63 characters at 50 ms per key, with Tab between
   them, then Tab and Space to check the terms.
6. Press Enter to sign up through the backend, and wait 1 s.

Main thread, script, layout and style times come from Chrome's
`Performance.getMetrics`, the processes' CPU time from
`SystemInfo.getProcessInfo` (in steps of 10 ms), private memory from
`/proc/<pid>/smaps_rollup`, the JS heap after a garbage collection, and
the GPU memory from a `memory-infra` memory dump. In a check run, the
two apps' screens matched after steps 4 to 6, with at most 69 pixels
differing, under the e2e tests' limit of 100.

CPU time in milliseconds, React / WebAssembly, with GPU raster at 1×:

| Step | Main thread | Script | Layout and style | Page's renderer process | GPU process |
| --- | --- | --- | --- | --- | --- |
| 1. Load | 73 / 64 | 37 / 24 | 3.6 / 0.5 | 120 / 100 | 70 / 60 |
| 2. Idle | 0.3 / 1.9 | 0 / 0.3 | 0 / 0 | 0 / 0 | 20 / 20 |
| 3. Idle, field focused | 5.7 / 24 | 0 / 16 | 0 / 0 | 10 / 40 | 50 / 100 |
| 4. Empty submit | 6.2 / 8.3 | 3.1 / 5.6 | 0.6 / 0.1 | 10 / 20 | 10 / 10 |
| 5. Typing | 130 / 178 | 27 / 117 | 11.6 / 0.4 | 220 / 290 | 180 / 410 |
| 6. Sign-up | 12 / 10 | 5.1 / 5.5 | 1.8 / 0 | 10 / 10 | 10 / 20 |

- **Loading costs about the same.** The canvas app runs less script
  (24 ms against 37 ms for React's bundle and hydration) and no layout.
- **Typing costs the canvas app more.** Every key repaints the whole
  canvas from Rust: 4.3 times React's script time, 1.4 times its main
  thread time, and 2.3 times its GPU process time. The React app spends
  its time in Chrome's layout and style, which the canvas app skips.
- **A focused field costs the canvas app CPU while idle.** The caret
  blink runs 16 ms of script and doubles the GPU process time every 10 s.
  React's caret, drawn by Chrome, runs no script.
- **A 2× screen costs the canvas app more GPU process time, not React.**
  At 2×, the GPU process time for steps 3 and 5 grows to 160 and 640 ms
  for the canvas app, and stays at 60 and 190 ms for React. The other
  times change by 20 ms at most.
- **Without a GPU, the canvas app costs much more.** With software
  raster, which headless Chrome and machines without a usable GPU use,
  Chrome paints the canvas on the page's main thread. Typing then takes
  700 ms of main thread time against React's 130 ms, and the page's
  renderer process 810 ms against 230 ms. A focused field takes 96 ms
  every 10 s against 5.6 ms.

Memory, React / WebAssembly, with GPU raster, after loading and after
signing up:

| Measure | After loading | After signing up |
| --- | --- | --- |
| Page's renderer, private memory | 17.5 / 17.7 MiB | 19.8 / 21.1 MiB |
| JS heap, used | 2.67 / 1.52 MiB | 3.27 / 1.64 MiB |
| WebAssembly memory | – / 1.19 MiB | – / 1.19 MiB |
| DOM nodes | 114 / 65 | 66 / 65 |
| JS event listeners | 162 / 26 | 157 / 26 |

GPU memory, React / WebAssembly, from the memory dump after typing a
name:

| Allocation | At 1× | At 2× |
| --- | --- | --- |
| Skia's GPU resources, in the GPU process | 8.5 / 52 MiB | 8.5 / 147 MiB |
| Shared images, in the GPU process | 10–12 / 15–20 MiB | 10 / 53 MiB |
| The canvas, in the page's renderer | – / 12.5–16.7 MiB | – / 50 MiB |

These allocations can overlap, so they do not add up to a total.

- **The canvas app's JavaScript side is no bigger.** Its JS heap and
  WebAssembly memory together match React's JS heap after loading,
  2.7 MiB, and are 2.8 MiB against 3.3 MiB after signing up; its
  WebAssembly memory did not grow in these steps. It has a sixth of
  React's event listeners, and 65 DOM nodes against React's 114 to 118
  while the form shows.
- **Its pixels cost GPU memory.** Skia's GPU resources are 6 times
  React's at 1× and 17 times at 2×, where Chrome holds 50 MiB for a canvas
  whose pixels take 16.7 MiB. Memory footprints hide this: the page's
  renderer has only 1.3 MiB more private memory than React's.
- **Without a GPU, the canvas is in the tab's memory.** With software
  raster, the page's renderer has 22.7 MiB of private memory after loading
  against React's 18.6 MiB. The 4.1 MiB difference is about the size of
  the 1280 × 856 canvas at 1×, 4.2 MiB.

These numbers come from one machine, with Chrome headless, and will
differ elsewhere. Each difference called out above held in every run.
