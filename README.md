# FatSecret MCP Server (Rust)

> [!IMPORTANT]
> This is **not an official MCP server** by FatSecret.
> It uses the [FatSecret Platform API](https://platform.fatsecret.com/), which requires a free developer account.

A multi-tenant [MCP](https://modelcontextprotocol.io) server for the FatSecret Platform API, written in Rust. It runs as a remote HTTP service. Each user enters their own FatSecret app credentials on a consent page, authorizes their FatSecret account once, and every later MCP tool call operates on that user's FatSecret data. The server stores nothing.

## Architecture

This is **not** a stdio CLI. It is an [axum](https://github.com/tokio-rs/axum) HTTP service that:

- Acts as a **stateless OAuth 2.1 Authorization Server** for MCP clients (Claude.ai, MCP Inspector) with PKCE (S256), dynamic client registration (`/register`), and refresh tokens.
- Seals every code, access token, refresh token, client id and in-flight flow state with **ChaCha20-Poly1305** (key derived from `OAUTH_SECRET`, blob type bound as AAD). The access token carries the user's FatSecret app credentials (client id, client secret, consumer secret) and their OAuth 1.0a access token/secret; `/mcp` decrypts it per request and builds the FatSecret client from it. No per-user storage, and tokens survive restarts as long as `OAUTH_SECRET` is stable.
- Has **no login gate**: access is tied to the FatSecret app credentials each user brings (see Security).
- Serves the FatSecret API as 42 MCP tools (built on [rmcp](https://crates.io/crates/rmcp)), scoped to the user making the request.

```
MCP client
   |  POST /mcp                                  -> 401 + WWW-Authenticate
   |  GET /.well-known/oauth-protected-resource  (server is its own AS)
   |  GET /.well-known/oauth-authorization-server
   |  POST /register                             -> sealed client_id
   |  GET /authorize?client_id=...&code_challenge=...
   v
Consent page: user enters FatSecret Client ID, Client Secret, Consumer Secret
   |  POST /authorize  (sealed flow cookie set)
   v
FatSecret OAuth 1.0a: request_token -> FatSecret authorize page
   |  -> /fatsecret/callback  (exchange verifier for access token)
   +-> mint sealed authorization code -> redirect to client
   |
   |  POST /token (code + PKCE verifier)  -> sealed access token (+ refresh token)
   |  POST /mcp with Bearer token
   v
Tools run with that user's FatSecret credentials
```

**Flow state.** While the user is bouncing through FatSecret, the PKCE/redirect info, the entered app credentials and the FatSecret request-token secret live in a sealed, `HttpOnly`, `SameSite=Lax` cookie (`fs_oauth_pending`, 10 minute TTL), not in the `oauth_callback` query string. This keeps the credentials out of URLs and logs and does not rely on FatSecret preserving a query string on its callback. The flow therefore must finish in the same browser it started in.

## Endpoints

| Method | Path | Auth | Purpose |
|---|---|---|---|
| GET | `/health` | none | Liveness probe |
| GET | `/.well-known/oauth-protected-resource` (and `/*rest`) | none | RFC 9728 protected resource metadata |
| GET | `/.well-known/oauth-authorization-server` (and `/*rest`) | none | RFC 8414 AS metadata |
| GET | `/.well-known/openid-configuration` (and `/*rest`) | none | Alias of the AS metadata |
| POST | `/register` | none | Dynamic client registration (redirect URIs must be allowlisted) |
| GET / POST | `/authorize` | none | Validates the request, shows the consent page, starts the FatSecret link |
| GET | `/fatsecret/callback` | flow cookie | Exchanges FatSecret verifier, mints auth code |
| POST | `/token` | PKCE / refresh token | Issues access (1 h) and refresh (90 d) tokens |
| any | `/mcp` and `/` | Bearer | MCP Streamable HTTP transport |

## Environment

Copy `.env.example` and fill in the values. Every variable is optional unless noted; the server does not load `.env` itself, so export the variables (or use your shell/Docker/Railway to inject them). There are no `FATSECRET_*` variables: each user supplies their own app credentials on the consent page.

| Variable | Purpose |
|---|---|
| `OAUTH_SECRET` | Key for sealing codes, tokens and client ids (`openssl rand -hex 32`). If unset a random one is generated with a warning and every token is invalidated on restart. Rotating it invalidates all tokens |
| `PORT` | HTTP listen port, default `8080` |
| `PUBLIC_HOST` | Public hostname for OAuth URLs; otherwise derived from `X-Forwarded-Host`/`Host` and `X-Forwarded-Proto` |
| `OAUTH_ALLOWED_REDIRECT_URIS` | Redirect URIs accepted at `/register` and for the static client. An entry ending in `/*` matches any URI with that prefix (scheme and host stay exact). Defaults to the Claude.ai/.com callback URLs, Google's `oauth-redirect[-sandbox|-test].googleusercontent.com/*` (Gemini) plus loopback URLs (`http://localhost`, `127.0.0.1`, `[::1]`, any port) for native clients like Gemini CLI; setting this variable replaces the defaults, loopback included; add others (e.g. MCP Inspector) here |
| `OAUTH_CLIENT_ID`, `OAUTH_CLIENT_SECRET` | Optional pre-registered client. Not needed with dynamic registration |

## Setup

1. **FatSecret credentials** (each user): [platform.fatsecret.com](https://platform.fatsecret.com/) -> My Account -> API Keys. Grab the Client ID, Client Secret and Consumer Secret; you paste them on the consent page when connecting.
2. **Connect from Claude.ai**: Connectors -> Add custom connector and set the URL to your server. Claude registers itself dynamically; you land on the consent page, then FatSecret authorization, then back.

## Tools (42)

**Auth**: `check_auth_status`

**Food search (public)**: `search_foods`, `get_food`, `find_food_by_barcode`, `autocomplete_foods`

**Recipes (public)**: `search_recipes`, `get_recipe`

**Reference (public)**: `get_food_categories`, `get_food_sub_categories`, `get_brands`, `get_recipe_types`

**Food diary (per user)**: `get_food_entries`, `get_food_entries_month`, `create_food_entry`, `edit_food_entry`, `delete_food_entry`, `copy_food_entries`, `copy_saved_meal_entries`, `save_weekly_food_diary` (whole weekly plan → diary; `overwrite` flag; retries rate limits after 30 s; streams "A of B meals processed" progress)

**Favorites (per user)**: `get_favorite_foods`, `delete_favorite_food`, `get_most_eaten_foods`, `get_recently_eaten_foods`, `get_favorite_recipes`, `add_favorite_recipe`, `delete_favorite_recipe`

**Saved meals (per user)**: `get_saved_meals`, `create_saved_meal`, `edit_saved_meal`, `delete_saved_meal`, `get_saved_meal_items`, `add_saved_meal_item`, `edit_saved_meal_item`, `delete_saved_meal_item`

**Weight (per user)**: `update_weight`, `get_weight_month`

**Exercise (per user)**: `get_exercises`, `edit_exercise_entries`, `get_exercise_entries_month`, `save_exercise_template`

**Profile (per user)**: `get_profile`, `create_food` (Premier)

API reference: [FatSecret Postman Collection](https://www.postman.com/fatsecret/fatsecret-public-apis/).

## Notes

- **MCP transport is stateless.** The Streamable HTTP service runs without sessions and replies with plain JSON (no SSE stream). Every request is authenticated by its Bearer token, so no `Mcp-Session-Id` is needed.
- **Stateless, no revocation list.** Tokens are self-contained, so there is nothing to revoke individually; rotate `OAUTH_SECRET` to invalidate everything. Auth codes expire after 5 minutes but, as there is no server-side store, are not tracked as single-use (PKCE binds them to the client that started the flow).
- **FatSecret tokens in the bearer.** The access token is encrypted, not hashed: anyone holding it can use that user's FatSecret account, and the server operator (who holds `OAUTH_SECRET`) can decrypt it. Access tokens last 1 hour; the 90 day refresh token carries the same credentials.
- **Security: no gate.** There is no allowlist or login step, so anyone who can reach the server and has a FatSecret developer app can use it. That is by design: every request runs with the user's own FatSecret credentials (carried in their sealed token), so nobody can reach another user's data or spend your FatSecret quota, and access is tied to their own creds. Restrict network access yourself if you want a private instance.
- Not live-tested against FatSecret in CI: the FatSecret request/access token legs are exercised only manually.

## Development

```bash
cp .env.example .env
set -a; source .env; set +a
cargo run                      # listens on :8080
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

CI (`.github/workflows/ci.yml`) runs fmt, clippy and tests on every push and pull request.

## Docker

```bash
docker build -t fatsecret-mcp-server .
docker run --rm -p 8080:8080 --env-file .env fatsecret-mcp-server
```

The image is a multi-stage build ending in `gcr.io/distroless/cc-debian12`. `docker-publish.yml` pushes `ghcr.io/<owner>/fatsecret-mcp-server:latest` on every push to `main`. `railway.toml` configures a `/health` healthcheck for Railway.

## License

MIT, see [LICENSE](LICENSE).
