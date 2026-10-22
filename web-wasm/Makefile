# Builds the static site into dist/: WebAssembly bundle, page, font, favicon and SEO.
DIST := dist

.PHONY: build clean

build:
	wasm-pack build --target web --release --no-typescript --no-pack --out-dir $(DIST)/pkg
	cp static/index.html $(DIST)/index.html
	mkdir -p $(DIST)/fonts
	cp ../design/fonts/inter-latin.woff2 $(DIST)/fonts/
	cp ../design/favicon.svg $(DIST)/
	node ../seo/inject.mjs --app web-wasm --dir $(DIST)

clean:
	rm -rf $(DIST)
