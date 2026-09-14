# Agent SaaS Starter

Build a SaaS app with authentication and OAuth-protected tools for ChatGPT and Claude.

[Documentation](docs/README.md) · [Demo](docs/README.md#demo) · [Contributing](CONTRIBUTING.md)

## Getting started

Install Rust 1.94+, Bun 1.3+, OpenSSL, and PostgreSQL, then create the database configured in `.env`:

```sh
cp .env.example .env
bun install --frozen-lockfile
bun run keys:dev
bun run dev
```

Open [localhost:3000](http://localhost:3000). See the [quickstart](docs/quickstart.mdx) for database setup, ports, and configuration.

## Documentation

Read the [guides](docs/README.md) for authentication, tenant boundaries, MCP connections, and deployment on Jio.

The example exposes one API-backed random-number tool. See [tested flows and current limitations](docs/guides/validation.mdx).

## Contributing

Bug reports and pull requests are welcome. Read [Contributing](CONTRIBUTING.md) and our [Code of Conduct](CODE_OF_CONDUCT.md).

## Security

Report vulnerabilities privately using our [security policy](SECURITY.md).

## License

[MIT](LICENSE).
