# Tack — Footprint & Performance Benchmarks

_Last measured against `develop` at workspace version 0.1.0-beta.8._

Tack's pitch is "fast, single-binary, self-hosted." This page replaces that claim
with numbers you can reproduce. **Everything below is one process and one SQLite
file — no Postgres, no Redis, no Docker.**

## Test machine

| | |
| --- | --- |
| CPU | 12th Gen Intel Core i5-12600K (16 threads) |
| RAM | 62 GiB |
| OS | Linux 6.14 (x86_64) |
| Build | `cargo build --release -p tack-cli --features embed-spa` (LTO, `opt-level="z"`, stripped) |

Numbers are hardware-dependent; the point is the **order of magnitude** and that
you can re-run the method below on your own box.

## Footprint

| Metric | Value | How measured |
| --- | --- | --- |
| **Binary size** (UI embedded) | **21.0 MiB** | `ls -l target/release/tack` — single file, SPA embedded via `embed-spa`, stripped by the release profile |
| **Binary size** (no UI) | 20.3 MiB | the same release build without `embed-spa` |
| **Cold start** (launch → first `/api/health` 200) | **~150 ms** | wall-clock poll loop, fresh empty DB; 144–256 ms across four runs |
| **Idle RSS** (resident memory, settled) | **~19.5 MiB** | `VmRSS` from `/proc/<pid>/status`, 1 s after ready |

The two size rows are 0.7 MiB apart, which is the whole web UI. The binary is twice
what it was before the execution fleet existed, and the UI is not why: the runner
protocol, the scheduler, the execution domain and their dependencies are. Embedding
the UI is nearly free; executing work is not.

For comparison, a typical Postgres-backed PM stack starts at hundreds of MiB of
resident memory across multiple containers before the app serves a request. Tack's
whole runtime is under 20 MiB.

## Request latency

Warm latency, measured client-side over loopback (sequential unless noted). The DB
held 100 items in one project.

| Endpoint | p50 | p95 | p99 |
| --- | --- | --- | --- |
| `GET /api/health` | 0.12 ms | 0.34 ms | 0.62 ms |
| `GET /api/projects/:id/items` (100 items) | 1.69 ms | 2.67 ms | 3.48 ms |
| `GET /api/projects/:id/search?q=…` (FTS5) | 1.13 ms | 2.82 ms | 3.82 ms |

Concurrent read throughput (32 client workers, `GET …/items`): **~786 req/s,
p99 ≈ 50 ms**. _Caveat: this number is bounded by the Python test client (GIL +
`urllib` overhead), not the server — treat it as a conservative floor, not a
ceiling._ For a rigorous throughput/saturation curve use the k6 baseline below.

## Reproduce it

Footprint + latency (no extra tooling needed):

```bash
# 1. Build the real single binary
cargo build --release -p tack-cli --features embed-spa
ls -l target/release/tack              # binary size

# 2. Run it from a scratch dir on an alternate port (so no tack.toml/tack.db interferes)
d=$(mktemp -d); cp target/release/tack "$d"/; cd "$d"
TACK_PORT=3211 TACK_LOG_LEVEL=warn ./tack serve &

# 3. Idle memory
awk '/VmRSS/{printf "%.1f MiB\n",$2/1024}' /proc/$!/status

# 4. Latency (sequential health probe)
python3 - <<'PY'
import urllib.request, time
u="http://127.0.0.1:3211/api/health"
for _ in range(50): urllib.request.urlopen(u).read()
t=[]
for _ in range(1000):
    s=time.perf_counter(); urllib.request.urlopen(u).read(); t.append((time.perf_counter()-s)*1000)
t.sort(); print(f"p50={t[499]:.2f}ms p99={t[989]:.2f}ms")
PY
```

> **Gotcha:** if a `tack.toml` exists in the working directory, it **overrides**
> `TACK_*` env vars (including `TACK_PORT`/`TACK_DATABASE_URL`). Always benchmark
> from a scratch directory, as above.

For load/saturation testing, use the committed k6 baseline:

```bash
make load     # tests/load/ — k6 scenario against a running server
```

## Notes & honesty

- Measured on one machine; your mileage varies with CPU, disk, and dataset size.
- SQLite is single-writer; these are **read-dominant** numbers. Write-heavy,
  highly-concurrent workloads are not Tack's target (it's built for solo devs and
  small teams — see the README positioning).
- The concurrent figure is client-limited (see caveat above); it is **not** a
  server saturation result. Replace it with a k6 run for a real ceiling.
