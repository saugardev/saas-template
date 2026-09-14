/** Destructive only to the disposable accounts this check creates. Run against a test deployment. */
import assert from "node:assert/strict";
import { createHash, randomBytes } from "node:crypto";

const api = process.env.API_PUBLIC_URL ?? "http://localhost:4400";
const resource = process.env.MCP_RESOURCE ?? "http://localhost:4401/mcp";
const suffix = randomBytes(8).toString("hex");
const password = randomBytes(24).toString("base64url");
let checks = 0;
async function request(
  path: string,
  method = "GET",
  body?: unknown,
  auth?: string,
  status = 200,
) {
  const form = body instanceof URLSearchParams;
  const response = await fetch(new URL(path, api), {
    method,
    redirect: "manual",
    signal: AbortSignal.timeout(15000),
    headers: {
      ...(body
        ? {
            "content-type": form
              ? "application/x-www-form-urlencoded"
              : "application/json",
          }
        : {}),
      ...(auth ? { authorization: auth } : {}),
    },
    body: body ? (form ? body : JSON.stringify(body)) : undefined,
  });
  assert.equal(
    response.status,
    status,
    `${method} ${path.split("?")[0]}: ${await response.clone().text()}`,
  );
  checks++;
  return response;
}
async function json(
  path: string,
  method = "GET",
  body?: unknown,
  auth?: string,
  status = 200,
) {
  const response = await request(path, method, body, auth, status);
  const text = await response.text();
  assert(text, `Empty JSON response: ${path}`);
  return JSON.parse(text);
}
const alice = await json(
  "/api/v1/auth/register",
  "POST",
  { name: "Review Alice", email: `alice-${suffix}@example.test`, password },
  undefined,
  201,
);
const bob = await json(
  "/api/v1/auth/register",
  "POST",
  { name: "Review Bob", email: `bob-${suffix}@example.test`, password },
  undefined,
  201,
);
const auth = `Session ${alice.session_token}`;
await request("/api/v1/me", "GET", undefined, undefined, 401);
await request(
  "/api/v1/auth/login",
  "POST",
  { email: alice.user.email, password: "wrong-password" },
  undefined,
  401,
);
await json("/api/v1/auth/login", "POST", {
  email: alice.user.email.toUpperCase(),
  password,
});
await request(
  "/api/v1/session/selection",
  "POST",
  { workspace_id: bob.workspace_id, project_id: bob.project_id },
  auth,
  403,
);
await request(
  `/api/v1/projects/${bob.project_id}/api-keys`,
  "POST",
  { name: "Forbidden key" },
  auth,
  403,
);
const workspace = await json(
  "/api/v1/workspaces",
  "POST",
  { name: "Review workspace" },
  auth,
  201,
);
const project = await json(
  "/api/v1/projects",
  "POST",
  { workspace_id: workspace.id, name: "Review project" },
  auth,
  201,
);
await request(
  "/api/v1/session/selection",
  "POST",
  { workspace_id: workspace.id, project_id: project.id },
  auth,
  204,
);
const key = await json(
  `/api/v1/projects/${project.id}/api-keys`,
  "POST",
  { name: "Review key" },
  auth,
  201,
);
assert.equal(
  (
    await json(
      "/api/v1/project-context",
      "GET",
      undefined,
      `ApiKey ${key.secret}`,
    )
  ).project_id,
  project.id,
);
await request(
  `/api/v1/api-keys/${key.api_key.id}`,
  "DELETE",
  undefined,
  auth,
  204,
);
await request(
  "/api/v1/project-context",
  "GET",
  undefined,
  `ApiKey ${key.secret}`,
  401,
);

const metadata = await json("/.well-known/oauth-authorization-server");
assert.deepEqual(metadata.code_challenge_methods_supported, ["S256"]);
const callback = "http://127.0.0.1:51987/callback";
const client = await json(
  "/oauth/register",
  "POST",
  {
    client_name: "Auth review",
    redirect_uris: [callback],
    token_endpoint_auth_method: "none",
  },
  undefined,
  201,
);
const verifier = randomBytes(48).toString("base64url");
const challenge = createHash("sha256").update(verifier).digest("base64url");
const query = new URLSearchParams({
  response_type: "code",
  client_id: client.client_id,
  redirect_uri: callback,
  scope: "openid profile email project:read offline_access",
  resource,
  state: suffix,
  code_challenge: challenge,
  code_challenge_method: "S256",
});
async function grant(approve = true) {
  const response = await request(
    `/oauth/authorize?${query}`,
    "GET",
    undefined,
    undefined,
    307,
  );
  const id = new URL(response.headers.get("location")!).searchParams.get(
    "request_id",
  );
  const path = `/api/v1/oauth/authorization-requests/${id}`;
  await request(path, "GET", undefined, undefined, 401);
  await json(path, "GET", undefined, auth);
  await request(
    path,
    "POST",
    {
      approve: true,
      workspace_id: bob.workspace_id,
      project_id: bob.project_id,
    },
    auth,
    403,
  );
  const result = await json(
    path,
    "POST",
    { approve, workspace_id: workspace.id, project_id: project.id },
    auth,
  );
  await request(
    path,
    "POST",
    { approve, workspace_id: workspace.id, project_id: project.id },
    auth,
    404,
  );
  const redirect = new URL(result.redirect_to);
  assert.equal(redirect.searchParams.get("state"), suffix);
  assert.equal(redirect.searchParams.get("iss"), api);
  if (!approve)
    assert.equal(redirect.searchParams.get("error"), "access_denied");
  return redirect.searchParams.get("code")!;
}
await grant(false);
const code = await grant();
const exchange = new URLSearchParams({
  grant_type: "authorization_code",
  client_id: client.client_id,
  code,
  redirect_uri: callback,
  code_verifier: verifier,
  resource,
});
const badVerifier = new URLSearchParams(exchange);
badVerifier.set("code_verifier", "x".repeat(64));
await request("/oauth/token", "POST", badVerifier, undefined, 400);
const tokens = await json("/oauth/token", "POST", exchange);
assert(tokens.refresh_token);
await request("/oauth/token", "POST", exchange, undefined, 400);
assert.equal(
  (
    await json(
      "/oauth/userinfo",
      "GET",
      undefined,
      `Bearer ${tokens.access_token}`,
    )
  ).sub,
  alice.user.id,
);
await request("/api/v1/random-number", "GET", undefined, undefined, 401);
const number = await json(
  "/api/v1/random-number",
  "GET",
  undefined,
  `Bearer ${tokens.access_token}`,
);
assert(
  Number.isInteger(number.number) && number.number >= 0 && number.number <= 100,
);
let session: string | null = null;
async function rpc(
  body: unknown,
  bearer = tokens.access_token,
  expected = 200,
) {
  const response = await fetch(resource, {
    method: "POST",
    signal: AbortSignal.timeout(15000),
    headers: {
      "content-type": "application/json",
      accept: "application/json, text/event-stream",
      ...(bearer ? { authorization: `Bearer ${bearer}` } : {}),
      ...(session
        ? { "mcp-session-id": session, "mcp-protocol-version": "2025-11-25" }
        : {}),
    },
    body: JSON.stringify(body),
  });
  assert.equal(
    response.status,
    expected,
    `MCP: ${await response.clone().text()}`,
  );
  session = response.headers.get("mcp-session-id") ?? session;
  checks++;
  if (expected !== 200) return response;
  const text = await response.text();
  assert(text, `Empty MCP response: ${JSON.stringify(body)}`);
  return JSON.parse(
    text.startsWith("event:") ||
      text.startsWith("id:") ||
      text.startsWith("data:")
      ? text
          .split("\n")
          .find(
            (line) =>
              line.startsWith("data:") && line.slice(5).trim().startsWith("{"),
          )!
          .slice(5)
      : text,
  );
}
const unauth = await rpc(
  { jsonrpc: "2.0", id: 0, method: "tools/list" },
  "",
  401,
);
assert(unauth.headers.get("www-authenticate")?.includes("resource_metadata="));
const init = await rpc({
  jsonrpc: "2.0",
  id: 1,
  method: "initialize",
  params: {
    protocolVersion: "2025-11-25",
    capabilities: {},
    clientInfo: { name: "auth-review", version: "1" },
  },
});
assert.equal(init.result.protocolVersion, "2025-11-25");
await rpc(
  { jsonrpc: "2.0", method: "notifications/initialized" },
  tokens.access_token,
  202,
);
const tools = await rpc({ jsonrpc: "2.0", id: 2, method: "tools/list" });
assert.deepEqual(
  tools.result.tools.map((tool: { name: string }) => tool.name),
  ["get_random_number"],
);
const result = await rpc({
  jsonrpc: "2.0",
  id: 3,
  method: "tools/call",
  params: { name: "get_random_number", arguments: {} },
});
assert.equal(result.result.isError, false);
assert(Number.isInteger(result.result.structuredContent.number));
// A refresh narrowed to offline_access must never regain project:read.
const narrowCode = await grant();
const narrowExchange = new URLSearchParams(exchange);
narrowExchange.set("code", narrowCode);
const broad = await json("/oauth/token", "POST", narrowExchange);
const narrowRequest = new URLSearchParams({
  grant_type: "refresh_token",
  client_id: client.client_id,
  refresh_token: broad.refresh_token,
  resource,
  scope: "offline_access",
});
const narrow = await json("/oauth/token", "POST", narrowRequest);
await request(
  "/api/v1/random-number",
  "GET",
  undefined,
  `Bearer ${narrow.access_token}`,
  401,
);
narrowRequest.set("refresh_token", narrow.refresh_token);
narrowRequest.delete("scope");
const stillNarrow = await json("/oauth/token", "POST", narrowRequest);
assert.equal(stillNarrow.scope, "offline_access");
await request(
  "/oauth/revoke",
  "POST",
  new URLSearchParams({
    token: stillNarrow.refresh_token,
    client_id: client.client_id,
  }),
);
narrowRequest.set("refresh_token", stillNarrow.refresh_token);
await request("/oauth/token", "POST", narrowRequest, undefined, 400);

const refresh = new URLSearchParams({
  grant_type: "refresh_token",
  client_id: client.client_id,
  refresh_token: tokens.refresh_token,
  resource,
});
const refreshed = await json("/oauth/token", "POST", refresh);
assert.notEqual(refreshed.refresh_token, tokens.refresh_token);
await request("/oauth/token", "POST", refresh, undefined, 400);
refresh.set("refresh_token", refreshed.refresh_token);
await request("/oauth/token", "POST", refresh, undefined, 400);
// Removed mail workflows must not remain callable.
for (const path of [
  "verification/request",
  "verification/confirm",
  "password/forgot",
  "password/reset",
])
  await request(`/api/v1/auth/${path}`, "POST", {}, undefined, 404);
assert.equal(
  (await json("/api/v1/me", "GET", undefined, auth)).email_verified,
  false,
);
// Seed only one-time tokens in the disposable test DB, then exercise real HTTP consumers.
if (process.env.TEST_DATABASE_URL) {
  const { SQL } = await import("bun");
  const db = new SQL(process.env.TEST_DATABASE_URL);
  const handoff = randomBytes(32).toString("base64url");
  await db`INSERT INTO session_handoffs(handoff_hash,user_id,workspace_id,project_id,expires_at,browser_challenge) VALUES(${createHash("sha256").update(handoff).digest()},${alice.user.id},${workspace.id},${project.id},now()+interval '1 minute',${challenge})`;
  await request(
    "/api/v1/auth/session-handoffs/consume",
    "POST",
    { handoff_code: handoff, browser_verifier: "x".repeat(43) },
    undefined,
    401,
  );
  await json("/api/v1/auth/session-handoffs/consume", "POST", {
    handoff_code: handoff,
    browser_verifier: verifier,
  });
  await request(
    "/api/v1/auth/session-handoffs/consume",
    "POST",
    { handoff_code: handoff, browser_verifier: verifier },
    undefined,
    400,
  );
  await db.close();
}
await request("/api/v1/auth/logout", "POST", undefined, auth, 204);
await request("/api/v1/me", "GET", undefined, auth, 401);
console.log(
  `Passed ${checks} HTTP checks: login, sessions, tenant isolation, API keys, PKCE, consent, MCP random-number tool, refresh rotation and replay rejection.`,
);
