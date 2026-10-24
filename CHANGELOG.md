# Changelog

## 1.0.0 (2026-10-24)


### Features

* **api:** define sign-up contract, copy and validation rules ([1e6299c](https://github.com/marcelomiyake/nodes-vs-pixels/commit/1e6299cd87353d1a0040a0455e4f559f4c893781))
* **backend:** serve sign-up over gRPC and gRPC-web ([bf0fbe3](https://github.com/marcelomiyake/nodes-vs-pixels/commit/bf0fbe348eb551b47ef7b38422d339e4ea93e5a2))
* **deploy:** run the system on Kind with a Helm chart ([131315e](https://github.com/marcelomiyake/nodes-vs-pixels/commit/131315eddfe8e273bbcf197293b076709529686d))
* **design:** add shared design tokens and Inter font ([90c524f](https://github.com/marcelomiyake/nodes-vs-pixels/commit/90c524f3ffd453711a60d11a8ad1bc6509eb0154))
* **form-core:** implement shared sign-up validation rules ([ebba396](https://github.com/marcelomiyake/nodes-vs-pixels/commit/ebba396ca2fc022a4c77ba7be5455acc1daf96ea))
* **seo:** add shared SEO metadata injection step ([7217a01](https://github.com/marcelomiyake/nodes-vs-pixels/commit/7217a0183be23c257729af5de679d873537d837b))
* **web-react:** build the sign-up form with gRPC-web submission ([aa7e0b3](https://github.com/marcelomiyake/nodes-vs-pixels/commit/aa7e0b3eb78136f4d60cb100ea5e666d5fb3173b))
* **web-react:** highlight selected text with the brand ring token ([bbbab02](https://github.com/marcelomiyake/nodes-vs-pixels/commit/bbbab0229d5e03254f96e956bf666eefad447f40))
* **web-react:** implement shared validation rules in TypeScript ([cf7b30a](https://github.com/marcelomiyake/nodes-vs-pixels/commit/cf7b30a52d76696058b572b727b143a0c438e98d))
* **web-react:** prerender the page, inline CSS and inject SEO ([40e550f](https://github.com/marcelomiyake/nodes-vs-pixels/commit/40e550f94e582d40bda20d65ab3b13567046d4b3))
* **web-wasm:** add the sign-up form state machine ([17eac4c](https://github.com/marcelomiyake/nodes-vs-pixels/commit/17eac4c01fa008a379eca31cb3c51a1c9bd6a548))
* **web-wasm:** add the unary gRPC-web codec ([21a82fd](https://github.com/marcelomiyake/nodes-vs-pixels/commit/21a82fd6f76d8042e22bfac5bcdb6e8c44b6d195))
* **web-wasm:** break canvas text where Chrome does ([8386c15](https://github.com/marcelomiyake/nodes-vs-pixels/commit/8386c15125141c0d9624b297159a138ec15d9ef9))
* **web-wasm:** delete emoji sequences whole on Backspace ([30bfc80](https://github.com/marcelomiyake/nodes-vs-pixels/commit/30bfc8018e37ca6ebc310028a60d8572cd0e6391))
* **web-wasm:** edit single-line text as Chrome's inputs do ([dd014a8](https://github.com/marcelomiyake/nodes-vs-pixels/commit/dd014a82740bf089787958ec9bb28b98b9d3636a))
* **web-wasm:** lay native controls over the canvas and handle events ([76337f2](https://github.com/marcelomiyake/nodes-vs-pixels/commit/76337f2253262e6b19574a6b3cb64936cefcf969))
* **web-wasm:** lay out the form like the Tailwind DOM ([bbf2f01](https://github.com/marcelomiyake/nodes-vs-pixels/commit/bbf2f01b6c28de61b4af8a94147e0f21c2ae6ef5))
* **web-wasm:** make the page one canvas that takes all input ([45e5963](https://github.com/marcelomiyake/nodes-vs-pixels/commit/45e5963f584174baa013e4373da4ada5f0a047e3))
* **web-wasm:** paint the form through a Painter ([54d37fc](https://github.com/marcelomiyake/nodes-vs-pixels/commit/54d37fce76ea3ecb8259bdb9742baf8648e8d794))
* **web-wasm:** place carets from canvas measurements ([e6c25ee](https://github.com/marcelomiyake/nodes-vs-pixels/commit/e6c25ee795caf08b0e5130ad2cbf4e596037204e))
* **web-wasm:** place the caret in mixed directions as Chrome does ([d30f348](https://github.com/marcelomiyake/nodes-vs-pixels/commit/d30f3489cfd286566acb5b05b39d4d0e8751ff77))
* **web-wasm:** run the canvas app in the browser ([12eb9bc](https://github.com/marcelomiyake/nodes-vs-pixels/commit/12eb9bc2debdc49d99ad9c32b4fa98b3bf5bc2c9))
* **web-wasm:** take keys and the clipboard on the canvas ([51871d4](https://github.com/marcelomiyake/nodes-vs-pixels/commit/51871d43cf47d87677f5964078e2fe598a09aa28))
* **web-wasm:** take the mouse on the canvas ([6f97bb3](https://github.com/marcelomiyake/nodes-vs-pixels/commit/6f97bb30382691649daa8aeccf7d42b698862b5d))
* **web-wasm:** undo and redo edits as Chrome's inputs do ([d90146d](https://github.com/marcelomiyake/nodes-vs-pixels/commit/d90146d35173d06048614b96aeb589735b54e70e))


### Bug Fixes

* **backend:** copy every workspace member into the image build ([34ffa51](https://github.com/marcelomiyake/nodes-vs-pixels/commit/34ffa511939215f15aeb05f3ddd09acfe3735fdf))
* **seo:** replace each page's title and stay inside the build directory ([d86b859](https://github.com/marcelomiyake/nodes-vs-pixels/commit/d86b859bbf5b864052679843c42c383ee909b54a))
* **web-react:** trim without a backtracking regular expression ([b74e42b](https://github.com/marcelomiyake/nodes-vs-pixels/commit/b74e42bef90c7263ca3339a540e381f05caf288e))
* **web-wasm:** import the module by a relative path ([4a4ff53](https://github.com/marcelomiyake/nodes-vs-pixels/commit/4a4ff53d7ce0347a69119c30406a40fb03fc38b1))
* **web-wasm:** paint boxes and the caret on whole pixels as Chrome does ([c60f8fc](https://github.com/marcelomiyake/nodes-vs-pixels/commit/c60f8fc32e80ba4270fb8e4cdfee1ba22cdde794))
* **web-wasm:** put the canvas in a main element ([c747096](https://github.com/marcelomiyake/nodes-vs-pixels/commit/c747096168148ef25e65fc68f0579a784951150c))
* **web-wasm:** scroll an input sideways with the wheel, as Chrome does ([6ebdd96](https://github.com/marcelomiyake/nodes-vs-pixels/commit/6ebdd96d1e4fd41917c53dec2d5ac8442797d16f))
* **web-wasm:** scroll the page as Chrome does while editing ([b259cff](https://github.com/marcelomiyake/nodes-vs-pixels/commit/b259cff61921eee83ab227fa01bebd6e717ae569))
* **web-wasm:** start Tab from the element clicked, as Chrome does ([24c63c0](https://github.com/marcelomiyake/nodes-vs-pixels/commit/24c63c039f73f8c3d767dbb335fd3f3a0498dcd3))


### Continuous Integration

* version, tag and release with release-please ([6400cb5](https://github.com/marcelomiyake/nodes-vs-pixels/commit/6400cb5f32ae84ea4c707bc5a8c167bd3ac01825))
