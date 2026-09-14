# Documentation

Build and run the starter, connect an agent, and understand the authentication boundary.

- [Quickstart](quickstart.mdx)
- [Architecture](architecture.mdx)
- [Configuration](configuration.mdx)
- [Authentication](guides/authentication.mdx)
- [MCP tools](guides/mcp.mdx)
- [ChatGPT](clients/chatgpt.mdx) · [Claude](clients/claude.mdx) · [Other clients](clients/other-clients.mdx)
- [Deploy on Jio](guides/jio-deployment.mdx)
- [API reference](reference/endpoints.mdx)
- [Test results and limitations](guides/validation.mdx)

## Demo

[Watch the walkthrough (MP4, 39 seconds)](../apps/docs/public/boilerplate.mp4).

The recording shows the application running on a Jio computer, followed by real API-backed MCP calls in ChatGPT and Claude. OAuth connections were configured beforehand. ChatGPT returns `13`; Claude returns `57` after one-time tool approval.

Run `bun run dev:docs` and open [the demo page](http://localhost:3003/docs/demo) to play the video in the documentation site.

## Development checks

```sh
bun run check
```

With the API and MCP running against a disposable database:

```sh
API_PUBLIC_URL=http://localhost:4000 MCP_RESOURCE=http://localhost:4001/mcp bun run test:auth
```

This creates uniquely named test accounts. Set `TEST_DATABASE_URL` to the same deployment's database to include browser-handoff checks. See [validation](guides/validation.mdx) for coverage and results.

## Repository layout

| Path | Purpose |
| --- | --- |
| `apps/app` | Browser application and consent UI |
| `apps/docs` | Documentation site, reading this folder |
| `apps/landing` | Public landing page |
| `services/api` | Authentication, tenancy, OAuth, and PostgreSQL |
| `services/mcp` | Protected Streamable HTTP MCP server |
| `crates` | Shared Rust authentication, database, and MCP code |

Edit the guides here; the documentation site uses the same MDX files. There is no separate copy to maintain.
