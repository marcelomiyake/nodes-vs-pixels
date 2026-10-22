# Builds, tests and deploys the whole system to a local Kind cluster.
CLUSTER := nodes-vs-pixels
CONTEXT := kind-$(CLUSTER)
TAG := 0.1.0
APPS := backend web-react web-wasm

.PHONY: build test coverage sonar images kind-up deploy kind-down

build:
	cargo build --release --locked -p backend
	pnpm --filter web-react build
	$(MAKE) -C web-wasm build

test:
	cargo clippy --workspace --all-targets --locked -- -D warnings
	cargo clippy -p web-wasm --target wasm32-unknown-unknown --locked -- -D warnings
	cargo test --workspace --locked
	pnpm --filter seo test
	pnpm --filter web-react lint
	pnpm --filter web-react typecheck
	pnpm --filter web-react test

# LCOV reports for SonarQube Cloud.
coverage:
	mkdir -p target/coverage
	cargo llvm-cov --locked -p backend -p form-core --lcov --output-path target/coverage/backend.lcov
	cargo llvm-cov --locked -p web-wasm --lcov --output-path target/coverage/web-wasm.lcov
	pnpm --filter seo test
	pnpm --filter web-react test

# Analyzes each project on SonarQube Cloud; needs SONAR_TOKEN.
sonar: coverage
	$(foreach app,$(APPS),sonar-scanner -Dproject.settings=$(app)/sonar-project.properties &&) true

images:
	$(foreach app,$(APPS),docker build -f $(app)/Dockerfile -t $(CLUSTER)/$(app):$(TAG) . &&) true

kind-up:
	kind get clusters | grep -qx $(CLUSTER) || kind create cluster --config deploy/kind/cluster.yaml

# Loads the images into the cluster, installs the chart and restarts the pods
# so that rebuilt images with the same tag are picked up.
deploy: images kind-up
	kind load docker-image --name $(CLUSTER) $(foreach app,$(APPS),$(CLUSTER)/$(app):$(TAG))
	helm upgrade --install $(CLUSTER) deploy/helm/$(CLUSTER) --kube-context $(CONTEXT) --wait
	kubectl --context $(CONTEXT) rollout restart deployment $(APPS)
	$(foreach app,$(APPS),kubectl --context $(CONTEXT) rollout status deployment/$(app) --timeout=120s &&) true

kind-down:
	kind delete cluster --name $(CLUSTER)

