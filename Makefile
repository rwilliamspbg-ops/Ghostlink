.PHONY: all build test clean docker-up docker-down bench run-api run-gui lint

# ─── Default ───
all: test build

# ─── Build ───
build:
	cargo build --release -p ghost-link -p ghostlink-core
	cd ghostlink_gui_modern && npm run build

# ─── Test ───
test:
	cargo test --release -p ghost-link -p ghostlink-core
	cd ghostlink_gui_modern && npm test
	cd control-plane && go test ./...

# ─── Docker ───
docker-up:
	docker compose up -d --build

docker-down:
	docker compose down

docker-logs:
	docker compose logs -f

# ─── Development ───
run-api:
	cargo run -p ghost-link -- serve 0.0.0.0 8003

run-gui:
	cd ghostlink_gui_modern && npm run dev

# ─── Benchmarks ───
bench:
	bash benchmarks/api-benchmark.sh

# ─── E2E ───
e2e:
	cd ghostlink_gui_modern && npx playwright test

# ─── Lint ───
lint:
	cargo clippy --all-targets -- -D warnings
	cd ghostlink_gui_modern && npx tsc --noEmit

# ─── Clean ───
clean:
	cargo clean
	cd ghostlink_gui_modern && rm -rf node_modules dist
