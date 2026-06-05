# WESTpunkte Prometheus Exporter

A Prometheus exporter for [WESTbahn](https://westbahn.at) loyalty points (WESTpunkte). It signs in to the customer-facing API with cookies copied from your browser, fetches your current balance and the next batch of points about to expire, and exposes the numbers as Prometheus metrics so Grafana can chart them and alert you before points are lost.

> Not affiliated with WESTbahn Management GmbH. This is an unofficial tool that uses your own browser session cookies to query the customer API.

## Metrics

| Metric | Type | Labels | Description |
| --- | --- | --- | --- |
| `westpunkte_balance` | gauge | — | Current WESTpunkte balance reported by the customer API. |
| `westpunkte_expiring` | gauge | `expires_on` (YYYY-MM-DD) | Points expiring on the next expiry date. |
| `westpunkte_expiry_days_remaining` | gauge | — | Days until the next batch expires. Negative if the date has already passed. |
| `westpunkte_fetch_success` | gauge | — | `1` if the most recent fetch from the WESTbahn API succeeded, otherwise `0`. |
| `westpunkte_last_attempt_timestamp_seconds` | gauge | — | Unix timestamp of the most recent fetch attempt. |
| `westpunkte_last_success_timestamp_seconds` | gauge | — | Unix timestamp of the most recent successful fetch. |

Suggested alerts:

```promql
# session token probably expired, refresh it from your browser
westpunkte_fetch_success == 0
  and ON() (time() - westpunkte_last_success_timestamp_seconds) > 3600

# points about to expire that you should use
westpunkte_expiring > 0 and westpunkte_expiry_days_remaining < 14
```

## Configuration

All configuration is via environment variables.

| Variable | Required | Default | Description |
| --- | --- | --- | --- |
| `WESTBAHN_AUTH_TOKEN` | yes | — | Value of the `customer_auth_token` cookie. |
| `WESTBAHN_AUTH_ID_EMAIL` | yes | — | Value of the `customer_auth_id_email` cookie (`<customer-id>:<email>`). |
| `PORT` | no | `9090` | TCP port the HTTP server listens on. |
| `REFRESH_INTERVAL_SECS` | no | `300` | How often to fetch from the WESTbahn API. |
| `REQUEST_TIMEOUT_SECS` | no | `30` | HTTP timeout for upstream calls. |
| `WESTBAHN_BASE_URL` | no | `https://westbahn.at` | Override the API host (used by tests). |
| `RUST_LOG` | no | `westpunkte_metrics=info` | `tracing` filter. |

### Obtaining the auth values

The WESTbahn website does not expose an API key. The exporter authenticates by replaying two browser cookies set after you sign in:

1. Open <https://westbahn.at> and sign in.
2. Open DevTools → *Storage* / *Application* → *Cookies* → `https://westbahn.at`.
3. Copy `customer_auth_token` → set as `WESTBAHN_AUTH_TOKEN`. **This rotates on every login; treat it as a password.**
4. Copy `customer_auth_id_email` → set as `WESTBAHN_AUTH_ID_EMAIL`. It is stable (`<customer-id>:<email>`) and only changes if your email changes.

When the session expires you'll see `westpunkte_fetch_success` drop to `0`; sign in again and update the secret.

## Running with Docker

```bash
docker run --rm -p 9090:9090 \
  -e WESTBAHN_AUTH_TOKEN="…" \
  -e WESTBAHN_AUTH_ID_EMAIL="123:you@example.com" \
  ghcr.io/yolgie/westpunkte-metrics:latest
```

Verify:

```bash
curl localhost:9090/metrics
```

## Running with Dockge / docker-compose

Drop the included [`docker-compose.yml`](./docker-compose.yml) into a new Dockge stack and set the two secrets via the Dockge UI (or via a `.env` file next to the compose file):

```env
WESTBAHN_AUTH_TOKEN=…
WESTBAHN_AUTH_ID_EMAIL=123:you@example.com
```

```yaml
services:
  westpunkte-metrics:
    image: ghcr.io/yolgie/westpunkte-metrics:latest
    container_name: westpunkte-metrics
    restart: unless-stopped
    ports:
      - "9090:9090"
    environment:
      WESTBAHN_AUTH_TOKEN: ${WESTBAHN_AUTH_TOKEN:?required}
      WESTBAHN_AUTH_ID_EMAIL: ${WESTBAHN_AUTH_ID_EMAIL:?required}
      REFRESH_INTERVAL_SECS: "300"
```

## Prometheus scrape config

```yaml
scrape_configs:
  - job_name: westpunkte
    scrape_interval: 5m
    static_configs:
      - targets: ['westpunkte-metrics:9090']
```

The exporter caches the upstream response between fetches (controlled by `REFRESH_INTERVAL_SECS`), so it is safe to scrape it more often than you fetch — only the configured refresh interval generates calls to the WESTbahn API.

## Endpoints

| Path | Purpose |
| --- | --- |
| `GET /metrics` | Prometheus exposition format. |
| `GET /health` | Liveness probe — always `200 OK` while the process is up. |
| `GET /` | Brief index page listing the endpoints above. |

The Docker image also accepts `--healthcheck`, which hits `GET /health` on `127.0.0.1:$PORT` and exits with status `0` / `1`. The included compose file wires this up as a Docker healthcheck.

## Local development

```bash
WESTBAHN_AUTH_TOKEN=… WESTBAHN_AUTH_ID_EMAIL=… cargo run
```

Then:

```bash
curl -s localhost:9090/metrics
```

## Testing

```bash
cargo test
```

The suite covers:

- **Unit tests** for config parsing, JSON response parsing (incl. expired session and malformed payload cases), and Prometheus output formatting (negative days, missing expiry, never-fetched state).
- **Integration tests** that spin up a [`wiremock`](https://docs.rs/wiremock) HTTP mock for the WESTbahn API, run the real [`axum`](https://docs.rs/axum) router on an ephemeral port, and assert that `/metrics` and `/health` behave correctly against fixture responses.

CI (`.github/workflows/ci.yml`) runs `cargo fmt --check`, `cargo clippy -D warnings`, and `cargo test` on every push.

## Releasing

Images are published to GHCR by `.github/workflows/docker.yml`:

- pushes to the default branch publish `:latest` and `:sha-<short>`
- pushes of a `v*` tag publish `:vX.Y.Z`

Build the image locally:

```bash
docker build -t westpunkte-metrics .
```

## License

[MIT](./LICENSE)
