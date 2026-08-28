# Agent SaaS Starter

A small, brand-neutral monorepo for building a multi-tenant SaaS product with a protected MCP server. It includes first-party user auth, generic social OpenID Connect, and an OAuth 2.1/OIDC authorization server that works with ChatGPT, Claude, and standards-compatible agent clients.

## What ships

- Next.js application for sign-in, consent, workspaces, projects, and API keys
- Axum API backed by PostgreSQL
- OAuth Authorization Code + S256 PKCE, CIMD, DCR, refresh rotation, JWKS, and UserInfo
- `rmcp` Streamable HTTP server with a protected example tool
- Fumadocs documentation and a minimal landing site

There is no product-specific business logic, blockchain integration, billing, TEE, provenance, or deployment infrastructure.

## Workspace

| Path | Purpose |
| --- | --- |
| `apps/app` | Browser BFF, auth UI, consent, and tenant dashboard |
| `apps/landing` | Minimal public landing page |
| `apps/docs` | Fumadocs guides and endpoint reference |
| `services/api` | Identity, tenancy, OAuth/OIDC, and PostgreSQL owner |
| `services/mcp` | Protected remote Streamable HTTP MCP server |
| `crates/auth`, `crates/db`, `crates/mcp` | Small reusable Rust boundaries |

## Local setup

1. Install Rust 1.94+, Bun 1.3+, OpenSSL, and PostgreSQL.
2. Copy `.env.example` to `.env` and create the configured database.
3. Run `bun install`.
4. Run `bun run keys:dev`.
5. Run `bun run dev`.

The API applies SQL migrations at startup. Development email messages are written to structured logs. The default ports are app `3000`, landing `3002`, docs `3003`, API `4000`, and MCP `4001`.

To connect an agent client, expose the API and MCP services over HTTPS, retain the relevant ChatGPT or Claude origin in `CIMD_ALLOWED_ORIGINS`, and add the exact `MCP_RESOURCE` URL in the client. The docs app contains client-specific walkthroughs.

## Checks

```bash
bun run check
```

Default Rust unit tests do not require PostgreSQL. Database integration tests run when `TEST_DATABASE_URL` is set.

## Security boundary

The Rust API owns identity and data. Next.js acts as a browser-facing BFF and stores only an opaque application session in an HttpOnly cookie. MCP access tokens are short-lived, audience-bound RS256 JWTs and are validated locally by the MCP resource server.
