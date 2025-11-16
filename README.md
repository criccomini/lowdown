# lowdown

This is a Rust reimplementation (inspired by
[`ivarref/mikkmokk-proxy`](https://github.com/ivarref/mikkmokk-proxy)) of an
unobtrusive reverse HTTP proxy that can inject faults between a client and a
backend service.

You can use it to explore and harden the resiliency of clients and backends by
simulating:

- failed requests (before the backend is called)
- failed responses (after the backend has processed the request)
- duplicate requests
- delayed requests (before calling the backend)
- delayed responses (after the backend responds)

All behavior is controlled through HTTP headers, environment variables, and a
small admin API.

> Note: this project is a clean-room rewrite in Rust, inspired by the original
> Clojure implementation and its behavior / docs. The original project is
> licensed under the Eclipse Public License 2.0; if you copy or combine code
> between the two, ensure you comply with that license.

---

## Quick start

### Run via Cargo

```bash
cargo run --release
```

By default this starts:

- the proxy server on `127.0.0.1:8080`
- the admin server on `127.0.0.1:7070`

You must either:

- set a default destination URL via environment:

  ```bash
  export DESTINATION_URL=http://example.com
  cargo run --release
  ```

- or use the path-based forwarding endpoints
  (`/lowdown-forward-http/...`, see below).

### Run via Docker

Build:

```bash
docker build -t lowdown .
```

Run (simple example, proxying to `http://example.com`):

```bash
docker run --rm --name lowdown \
  -e DESTINATION_URL=http://example.com \
  -e PROXY_BIND=0.0.0.0 \
  -e PROXY_PORT=8080 \
  -e ADMIN_BIND=0.0.0.0 \
  -e ADMIN_PORT=7070 \
  -p 8080:8080 \
  -p 7070:7070 \
  lowdown
```

Now:

- send regular traffic through `http://localhost:8080`
- manage settings via `http://localhost:7070` (admin API)

---

## How it works

High-level call flow:

1. Client sends an HTTP request to the proxy.
2. The proxy decides whether the request "matches" based on URI, method,
   host, and arbitrary header name/value.
3. If the request matches, the proxy may:
   - fail the request before reaching the backend (`fail-before`)
   - add a delay before calling the backend
   - send a duplicate request alongside the primary one
4. The proxy forwards the request to the backend (destination).
5. The proxy receives one or two backend responses.
6. After the backend responds, the proxy may:
   - fail the response (`fail-after`)
   - add an additional delay
7. The proxy returns the selected or synthesized response to the client.

Fault injection is probabilistic. Each `*-percentage` setting is interpreted as
the percentage chance in `[0, 100]` that the corresponding behavior activates
for a matching request.

---

## Configuration model

There are three layers of configuration, applied in this order:

1. **Built-in defaults** (hard-coded)
2. **Environment variables** (process-level defaults)
3. **Admin overrides** (mutable at runtime via admin API)
4. **Per-request overrides** (via `x-lowdown-*` headers)

At request time, a snapshot of the effective settings is built by merging these
layers. Additionally, **one-off rules** can consume themselves the first time a
matching request is seen (see below).

### Default values

These are the built-in defaults (before env/admin/headers are applied):

| Setting key              | Default |
|--------------------------|---------|
| `delay-after-ms`         | `0`     |
| `delay-after-percentage` | `0`     |
| `delay-before-ms`        | `0`     |
| `delay-before-percentage`| `0`     |
| `destination-url`        | `nil`   |
| `duplicate-percentage`   | `0`     |
| `fail-after-code`        | `502`   |
| `fail-after-percentage`  | `0`     |
| `fail-before-code`       | `503`   |
| `fail-before-percentage` | `0`     |
| `match-header-name`      | `*`     |
| `match-header-value`     | `*`     |
| `match-host`             | `*`     |
| `match-method`           | `*`     |
| `match-uri`              | `*`     |
| `match-uri-regex`        | `*`     |
| `match-uri-starts-with`  | `*`     |

Semantics:

- `*` means "match everything".
- `destination-url` of `nil` means "no default backend"; you must provide one
  via env, admin update, or per-request header.

---

## Per-request headers (`x-lowdown-*`)

When sending a request through the proxy, you can control its behavior using
headers:

- Actual HTTP header name: `x-lowdown-<setting-name>`
- Where `<setting-name>` is one of the keys above (e.g. `fail-before-percentage`)

Examples:

- Always fail before reaching the backend:

  ```bash
  curl -v \
    -H 'x-lowdown-destination-url: http://example.com' \
    -H 'x-lowdown-fail-before-percentage: 100' \
    http://localhost:8080/
  ```

- Inject a fixed delay before calling the backend:

  ```bash
  curl -v \
    -H 'x-lowdown-destination-url: http://example.com' \
    -H 'x-lowdown-delay-before-percentage: 100' \
    -H 'x-lowdown-delay-before-ms: 3000' \
    http://localhost:8080/
  ```

- Send duplicate requests:

  ```bash
  curl -v \
    -H 'x-lowdown-destination-url: http://example.com' \
    -H 'x-lowdown-duplicate-percentage: 100' \
    http://localhost:8080/
  ```

### Matching controls

Fault injection only applies if the request "matches" according to the
following settings (after merging env/admin/header/one-off layers):

- `match-uri`: exact match with the request path (e.g. `/foo/bar`)
- `match-uri-starts-with`: prefix match on the request path
- `match-uri-regex`: full regex match against the request path,
  e.g. `/api/uuid/([a-f0-9]{8}(-[a-f0-9]{4}){3}-[a-f0-9]{12})`
- `match-method`: HTTP method (e.g. `GET`, `POST`), case-insensitive
- `match-host`: backend host name (e.g. `example.org`), matched against
  the destination's host portion
- `match-header-name` / `match-header-value`:
  - if either is `*`, all requests match
  - otherwise, the request must contain a header whose (case-insensitive) name
    equals `match-header-name` and whose value equals `match-header-value`

Only if **all** matchers succeed will any `*-percentage` settings be considered.

### Percentages and randomness

For each percentage field (e.g. `fail-before-percentage`), when a request
matches:

- a random integer in `[0, 99]` is drawn
- if `percentage > random_value`, the behavior is triggered

This is intentionally equivalent to "percentage chance out of 100".

---

## Environment variables

Each setting key can also be provided via an environment variable:

- Uppercase
- Dashes replaced with underscores

For example:

- `destination-url` → `DESTINATION_URL`
- `fail-before-percentage` → `FAIL_BEFORE_PERCENTAGE`
- `match-uri-starts-with` → `MATCH_URI_STARTS_WITH`

These environment defaults are merged on top of the built-in defaults and
beneath admin/headers/one-off overrides.

Special non-behavior env vars:

- `PROXY_BIND`: IP/host to bind the proxy server (default `127.0.0.1`)
- `PROXY_PORT`: proxy port (default `8080`)
- `ADMIN_BIND`: IP/host to bind the admin server (default `127.0.0.1`)
- `ADMIN_PORT`: admin port (default `7070`)
- `LOWDOWN_DEVELOPMENT`: if set to `true`, JSON responses include a trailing
  newline to make terminal output nicer
- `TZ`: timezone for timestamps in logs (e.g. `Europe/Oslo`), depends on
  system support

---

## Path-based forwarding

You do **not** need a dedicated instance per backend. Instead, you can route to
arbitrary hosts using special path prefixes:

- `GET /lowdown-forward-http/{host}` → forwards to `http://{host}/`
- `GET /lowdown-forward-http/{host}/{path...}` → forwards to
  `http://{host}/{path...}`
- `GET /lowdown-forward-https/{host}/{path...}` → forwards to
  `https://{host}/{path...}`

Examples:

```bash
# Plain HTTP
curl http://localhost:8080/lowdown-forward-http/example.org/

# HTTPS with path
curl http://localhost:8080/lowdown-forward-https/example.org/api/health
```

Internally, the proxy converts these paths into a `x-lowdown-destination-url`
header and a normalized request URI, so they behave exactly like explicit
`x-lowdown-destination-url` usage.

---

## Header rewriting

When forwarding to the backend, the proxy adjusts:

- the `Host` header:
  - set to the destination host (and port, if present)
- the `Origin` header:
  - if present, rewritten to `scheme://host[:port]` of the destination

When returning the backend's response, if the backend sets
`Access-Control-Allow-Origin` and the client sent an `Origin`, the proxy
rewrites `Access-Control-Allow-Origin` to match the client's original `Origin`.

This matches the behavior of the original Clojure implementation and helps
with CORS-sensitive frontends.

---

## Admin API

The admin API runs on the `ADMIN_BIND:ADMIN_PORT` address (default
`127.0.0.1:7070`). It provides:

### `POST /api/v1/update`

Merge new defaults into the current admin settings, using the same
`x-lowdown-*` header schema.

Example:

```bash
curl -XPOST \
  -H 'x-lowdown-fail-before-percentage: 20' \
  -H 'x-lowdown-destination-url: http://example.com' \
  http://localhost:7070/api/v1/update
```

Returns the full effective settings (default + env + admin) as JSON.

### `POST /api/v1/reset`

Reset admin settings to an empty override layer, optionally seeding new values
from headers in this request.

Example:

```bash
curl -XPOST http://localhost:7070/api/v1/reset
```

Response is the same shape as `/api/v1/update`.

### `GET /api/v1/list`

Return the current admin override layer as JSON (merged with defaults/env).

```bash
curl http://localhost:7070/api/v1/list
```

### `POST /api/v1/one-off`

Create a one-off rule: a settings snapshot that will be applied to the **next
matching request only**, then discarded.

Example: fail the next request before reaching the backend:

```bash
curl -XPOST \
  -H 'x-lowdown-fail-before-percentage: 100' \
  http://localhost:7070/api/v1/one-off
```

- The next matching request through the proxy will fail with `fail-before`.
- After that, the rule is removed and behavior reverts to the previous
  effective settings.

Matching uses the same `match-*` semantics as regular requests, and
`destination-url` inside the one-off is derived from the current effective
settings at the time the rule is consumed.

### `POST /api/v1/list-headers`

Log all incoming headers (splitting `x-lowdown-*` and non-lowdown headers)
and return a JSON array of header names. This is useful for introspecting what
your gateway or client is actually sending.

```bash
curl -XPOST -H 'X-Foo: Bar' http://localhost:7070/api/v1/list-headers
```

### Service/health endpoints

- `GET /` → `{"service":"lowdown"}`
- `GET /health` and `GET /healthcheck` → `{"service":"lowdown","status":"healthy"}`

These are primarily for simple health and discovery checks.

---

## Logging

Logging is handled via `tracing` and `tracing-subscriber`.

- Configure via `RUST_LOG`, e.g.:

  ```bash
  RUST_LOG=info,lowdown=debug
  ```

- You will see logs for:
  - server startup (proxy/admin addresses)
  - environment-derived settings
  - added/consumed one-off rules
  - duplicate request status comparisons
  - delays (`before-delay`, `delay-after`)
  - fail-before / fail-after activations
  - header dumps for `/api/v1/list-headers`

If `TZ` is set appropriately in the container/host, timestamps will respect the
requested timezone (subject to OS support).

---

## Building and testing

Build a release binary:

```bash
cargo build --release
```

Run tests:

```bash
cargo test
```

Tests are written as integration-style tests around the axum routers with a
stub `HttpClient`, so they do not require external services. They verify:

- basic proxy forwarding behavior
- fail-before and fail-after semantics
- duplicate request behavior
- header rewrite behavior (Host/Origin / CORS)
- admin `update`/`reset` plumbing
- one-off rule consumption
- delay timings (within coarse bounds)

---

## Limitations

These limitations mirror the original project:

- No TLS/SSL on the proxy bind side (use a separate TLS terminator / ingress).
- No WebSocket or Server-Sent Events support.
- Percentages and status codes are not validated:
  - `*-percentage` should be in `[0, 100]`
  - `*-code` should be a valid HTTP status code (`[200, 600)`)
- This proxy is **not** intended for untrusted or public networks.
- It is **not** intended for production — use it as a testing / chaos
  engineering tool.

---

## Credits

- Original design and behavior:
  [`ivarref/mikkmokk-proxy`](https://github.com/ivarref/mikkmokk-proxy)
- This crate provides a Rust/axum-based implementation with similar semantics,
  suitable for environments where Rust is preferred for deployment or
  integration.***
