import "server-only";

import { cookies } from "next/headers";

export const SESSION_COOKIE = "starter_session";
const API_URL =
  process.env.API_INTERNAL_URL ??
  process.env.API_PUBLIC_URL ??
  "http://localhost:4000";

type ApiErrorBody = {
  error?: { code?: string; message?: string; request_id?: string };
};

export class ApiError extends Error {
  constructor(
    public readonly status: number,
    public readonly code: string,
    message: string,
  ) {
    super(message);
  }
}

export async function getSessionToken() {
  return (await cookies()).get(SESSION_COOKIE)?.value;
}

export async function api<T>(path: string, init: RequestInit = {}): Promise<T> {
  const session = await getSessionToken();
  const headers = new Headers(init.headers);
  headers.set("accept", "application/json");
  if (init.body) headers.set("content-type", "application/json");
  if (session) headers.set("authorization", `Session ${session}`);
  const response = await fetch(`${API_URL}${path}`, {
    ...init,
    headers,
    cache: "no-store",
  });
  if (!response.ok) {
    const body = (await response.json().catch(() => ({}))) as ApiErrorBody;
    throw new ApiError(
      response.status,
      body.error?.code ?? "request_failed",
      body.error?.message ?? "The request could not be completed",
    );
  }
  if (response.status === 204) return undefined as T;
  return response.json() as Promise<T>;
}

export function safeReturnTo(
  value: FormDataEntryValue | string | null | undefined,
) {
  const target = typeof value === "string" ? value : "/dashboard";
  return target.startsWith("/") &&
    !target.startsWith("//") &&
    !/[\\\x00-\x20\x7f]/.test(target)
    ? target
    : "/dashboard";
}
