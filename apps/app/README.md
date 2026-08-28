# Product app

The browser-facing Next.js BFF for sign-in, email recovery, OAuth consent, workspace/project selection, and API-key management.

Run it through the repository root so it receives the shared environment:

```bash
bun run dev:app
```

The Rust API remains the source of truth. This app stores only the opaque application session in an HttpOnly cookie.
