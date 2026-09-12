.PHONY: build run dev debug cli test test-verbose test-core test-db e2e e2e-install e2e-ui screenshots audit load check lint fmt fmt-check reset-db inspect-db api-health api-stats api-projects clean clean-all desktop-sidecar desktop changelog changelog-release help

# ─── Default ──────────────────────────────────────
help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-15s\033[0m %s\n", $$1, $$2}'

# ─── Building & Running ───────────────────────────
build: ## Compile — frontend + single `tack` binary with embedded UI (~30s first time)
	npm --prefix frontend ci
	npm --prefix frontend run build
	cargo build -p tack-cli --release --features embed-spa
	@echo ""
	@echo "  Ready. Start with: make run"

run: ## Start the pre-built binary
	./target/release/tack

desktop-sidecar: ## Build `tack` and stage it as tack-desktop's Tauri sidecar
	npm --prefix frontend ci
	npm --prefix frontend run build
	cargo build -p tack-cli --release --features embed-spa
	mkdir -p crates/tack-desktop/binaries
	cp "$${CARGO_TARGET_DIR:-target}/release/tack" crates/tack-desktop/binaries/tack-$$(rustc --print host-tuple)

# `externalBin` lives in tauri.bundle.conf.json, not tauri.conf.json, so a plain
# `cargo build --workspace` never demands a staged sidecar. Bundling does, and
# gets it by merging the overlay here.
desktop: desktop-sidecar ## Bundle the desktop app (stages the sidecar first)
	cd crates/tack-desktop && cargo tauri build --config tauri.bundle.conf.json

dev: frontend/node_modules ## Development mode: server + Vite hot-reload (Ctrl-C stops both)
	@trap 'kill 0' SIGINT; \
	cargo run -p tack-cli -- serve & \
	npm --prefix frontend run dev & \
	wait

frontend/node_modules:
	npm --prefix frontend install

debug: ## Start the server with verbose logging
	RUST_LOG=tack_api=debug,tack_db=debug,tower_http=debug cargo run -p tack-cli -- serve

cli: ## Run the CLI (use ARGS="..." to pass arguments)
	cargo run --bin tack -- $(ARGS)

# ─── Testing ─────────────────────────────────────
test: ## Run all tests (nextest: failures + one summary line)
	cargo nextest run --workspace

test-verbose: ## Run all tests showing their output (serial)
	cargo nextest run --workspace --no-capture

test-core: ## Run only core unit tests
	cargo nextest run --workspace -E 'package(tack-core)'

test-db: ## Run only database integration tests
	cargo nextest run --workspace -E 'package(tack-db)'

# ─── End-to-End (browser) ────────────────────────
e2e-install: frontend/node_modules ## Install Playwright browsers (one-time)
	npm --prefix frontend exec playwright install --with-deps

e2e: frontend/node_modules ## Run E2E tests (starts API + Vite automatically, all browsers)
	npm --prefix frontend run test:e2e

e2e-ui: frontend/node_modules ## Run E2E tests in the interactive Playwright UI
	npm --prefix frontend run test:e2e:ui

screenshots: frontend/node_modules ## Capture README screenshots → docs/screenshots/ (starts API + Vite automatically)
	cd frontend && npx playwright test e2e/screenshots.spec.ts --config playwright.capture.config.ts --project=chromium --workers=1

# hero.gif / agents-flow.gif are real, live, billed recordings against a
# release build with real installed agents — there is no `make` target for
# them. See the recipe in frontend/e2e/agent-assets.spec.ts's own header.

# ─── Security & Performance ──────────────────────
audit: ## Scan Rust + npm dependencies for known CVEs (both workspaces + frontend)
	@command -v cargo-audit >/dev/null 2>&1 || { echo "Installing cargo-audit..."; cargo install cargo-audit --locked; }
	cargo audit
	cargo audit --file crates/tack-desktop/Cargo.lock
	npm --prefix frontend audit --audit-level=high

load: ## Run the k6 load test (requires a running API on :3210 and k6 installed)
	@command -v k6 >/dev/null 2>&1 || { echo "k6 not installed — see tests/load/README.md"; exit 1; }
	k6 run tests/load/smoke.js

# ─── Code Quality ────────────────────────────────
check: ## Type-check without building
	cargo check --workspace

lint: ## Run clippy linter (matches CI/pre-push: --all-targets covers tests too)
	cargo clippy --workspace --all-targets -- -D warnings

fmt: ## Format all Rust code
	cargo fmt --all

fmt-check: ## Check formatting (used in CI)
	cargo fmt --all -- --check

coverage: ## Rust + frontend coverage against CI's thresholds (see ci.yml's `coverage` job)
	@command -v cargo-llvm-cov >/dev/null 2>&1 || { echo "Installing cargo-llvm-cov..."; cargo install cargo-llvm-cov --locked; }
	cargo llvm-cov -p tack-core --fail-under-lines 85
	cargo llvm-cov -p tack-db --fail-under-lines 70
	cargo llvm-cov -p tack-api --fail-under-lines 70
	cargo llvm-cov -p tack-orch --fail-under-lines 70
	cargo llvm-cov -p tack-runner --fail-under-lines 85
	@cd frontend && npm ls @vitest/coverage-v8 >/dev/null 2>&1 || npm install --no-save @vitest/coverage-v8@^4
	cd frontend && npx vitest run --coverage --coverage.provider=v8 \
		--coverage.thresholds.lines=70 --coverage.thresholds.functions=70 \
		--coverage.thresholds.statements=70 --coverage.thresholds.branches=60

changelog: ## Preview the next release's CHANGELOG section from conventional commits since the last tag (git-cliff)
	@command -v git-cliff >/dev/null 2>&1 || { echo "Installing git-cliff..."; cargo install git-cliff --locked; }
	git-cliff --unreleased --strip all

changelog-release: ## Write TAG's section into CHANGELOG.md, e.g. `make changelog-release TAG=v0.1.0-beta.9` — run it, commit, then tag
	@test -n "$(TAG)" || { echo "usage: make changelog-release TAG=vX.Y.Z"; exit 2; }
	@command -v git-cliff >/dev/null 2>&1 || { echo "Installing git-cliff..."; cargo install git-cliff --locked; }
	git-cliff --unreleased --tag "$(TAG)" --prepend CHANGELOG.md
	@echo "  CHANGELOG.md now has the $(TAG) section — review it, commit, then: git tag $(TAG)"

deny: ## Dependency policy check, both workspaces (policy generated to match ci.yml's `deny` job exactly)
	@command -v cargo-deny >/dev/null 2>&1 || { echo "Installing cargo-deny..."; cargo install cargo-deny --locked; }
	@./scripts/gen-deny-toml.sh
	cargo deny check
	cargo deny --manifest-path crates/tack-desktop/Cargo.toml check

# ─── Database ────────────────────────────────────
reset-db: ## Delete the database (auto-recreated on next run)
	rm -f tack.db tack.db-shm tack.db-wal
	@echo "Database deleted. Run 'make run' or 'make dev' to recreate."

inspect-db: ## Open the live database in SQLite CLI
	sqlite3 tack.db

# ─── Quick API Checks ────────────────────────────
api-health: ## Check server health
	@curl -s http://localhost:3210/api/health | python3 -m json.tool 2>/dev/null || curl -s http://localhost:3210/api/health

api-stats: ## Show database statistics
	@curl -s http://localhost:3210/api/debug/db-stats | python3 -m json.tool 2>/dev/null || curl -s http://localhost:3210/api/debug/db-stats

api-projects: ## List all projects
	@curl -s http://localhost:3210/api/projects | python3 -m json.tool 2>/dev/null || curl -s http://localhost:3210/api/projects

# ─── Cleanup ─────────────────────────────────────
clean: ## Remove build artifacts
	cargo clean

clean-all: clean reset-db ## Remove build artifacts and database
