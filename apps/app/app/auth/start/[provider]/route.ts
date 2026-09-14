import { createHash, randomBytes } from "node:crypto";
import { NextRequest, NextResponse } from "next/server";
import { safeReturnTo } from "@/lib/api";

export async function GET(
  request: NextRequest,
  context: { params: Promise<{ provider: string }> },
) {
  const { provider } = await context.params;
  if (!/^[a-z0-9-]{1,48}$/.test(provider))
    return new NextResponse("Unknown provider", { status: 404 });
  const verifier = randomBytes(32).toString("base64url");
  const target = new URL(
    `/api/v1/auth/oidc/${provider}/start`,
    process.env.API_PUBLIC_URL ?? "http://localhost:4000",
  );
  target.searchParams.set(
    "return_to",
    safeReturnTo(request.nextUrl.searchParams.get("return_to")),
  );
  target.searchParams.set(
    "browser_challenge",
    createHash("sha256").update(verifier).digest("base64url"),
  );
  const response = NextResponse.redirect(target);
  response.cookies.set("starter_oidc_verifier", verifier, {
    httpOnly: true,
    secure: process.env.NODE_ENV === "production",
    sameSite: "lax",
    path: "/auth",
    maxAge: 600,
  });
  return response;
}
