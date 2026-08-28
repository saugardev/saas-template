import { NextRequest, NextResponse } from "next/server";
import { safeReturnTo, SESSION_COOKIE } from "@/lib/api";

const API_URL =
  process.env.API_INTERNAL_URL ??
  process.env.API_PUBLIC_URL ??
  "http://localhost:4000";

export async function GET(request: NextRequest) {
  const handoffCode = request.nextUrl.searchParams.get("handoff_code");
  const returnTo = safeReturnTo(request.nextUrl.searchParams.get("return_to"));
  if (!handoffCode) {
    return NextResponse.redirect(new URL("/login?error=invalid_handoff", request.url));
  }
  const apiResponse = await fetch(
    `${API_URL}/api/v1/auth/session-handoffs/consume`,
    {
      method: "POST",
      headers: { "content-type": "application/json", accept: "application/json" },
      body: JSON.stringify({ handoff_code: handoffCode }),
      cache: "no-store",
    },
  );
  if (!apiResponse.ok) {
    return NextResponse.redirect(new URL("/login?error=invalid_handoff", request.url));
  }
  const result = (await apiResponse.json()) as { session_token: string };
  const response = NextResponse.redirect(new URL(returnTo, request.url));
  response.cookies.set(SESSION_COOKIE, result.session_token, {
    httpOnly: true,
    secure: process.env.NODE_ENV === "production",
    sameSite: "lax",
    path: "/",
    maxAge: 30 * 24 * 60 * 60,
  });
  return response;
}

