"use server";

import { revalidatePath } from "next/cache";
import { cookies } from "next/headers";
import { redirect } from "next/navigation";
import { api, ApiError, safeReturnTo, SESSION_COOKIE } from "@/lib/api";

export type ActionState = {
  error?: string;
  success?: string;
  secret?: string;
};

type AuthResponse = { session_token: string };

async function setSession(token: string) {
  (await cookies()).set(SESSION_COOKIE, token, {
    httpOnly: true,
    secure: process.env.NODE_ENV === "production",
    sameSite: "lax",
    path: "/",
    maxAge: 30 * 24 * 60 * 60,
  });
}

function message(error: unknown) {
  return error instanceof ApiError
    ? error.message
    : "The server is unavailable. Try again.";
}

export async function loginAction(
  _state: ActionState,
  formData: FormData,
): Promise<ActionState> {
  const returnTo = safeReturnTo(formData.get("return_to"));
  try {
    const result = await api<AuthResponse>("/api/v1/auth/login", {
      method: "POST",
      body: JSON.stringify({
        email: formData.get("email"),
        password: formData.get("password"),
      }),
    });
    await setSession(result.session_token);
  } catch (error) {
    return { error: message(error) };
  }
  redirect(returnTo);
}

export async function registerAction(
  _state: ActionState,
  formData: FormData,
): Promise<ActionState> {
  try {
    const result = await api<AuthResponse>("/api/v1/auth/register", {
      method: "POST",
      body: JSON.stringify({
        name: formData.get("name"),
        email: formData.get("email"),
        password: formData.get("password"),
      }),
    });
    await setSession(result.session_token);
  } catch (error) {
    return { error: message(error) };
  }
  redirect("/dashboard?welcome=1");
}

export async function logoutAction() {
  try {
    await api("/api/v1/auth/logout", { method: "POST" });
  } finally {
    (await cookies()).delete(SESSION_COOKIE);
  }
  redirect("/login");
}

export async function forgotPasswordAction(
  _state: ActionState,
  formData: FormData,
): Promise<ActionState> {
  try {
    await api("/api/v1/auth/password/forgot", {
      method: "POST",
      body: JSON.stringify({ email: formData.get("email") }),
    });
    return {
      success: "If that account exists, a reset link has been sent.",
    };
  } catch (error) {
    return { error: message(error) };
  }
}

export async function resetPasswordAction(
  _state: ActionState,
  formData: FormData,
): Promise<ActionState> {
  try {
    await api("/api/v1/auth/password/reset", {
      method: "POST",
      body: JSON.stringify({
        token: formData.get("token"),
        password: formData.get("password"),
      }),
    });
  } catch (error) {
    return { error: message(error) };
  }
  redirect("/login?reset=1");
}

export async function verifyEmailAction(
  _state: ActionState,
  formData: FormData,
): Promise<ActionState> {
  try {
    await api("/api/v1/auth/verification/confirm", {
      method: "POST",
      body: JSON.stringify({ token: formData.get("token") }),
    });
    return { success: "Email verified. You can return to the dashboard." };
  } catch (error) {
    return { error: message(error) };
  }
}

export async function resendVerificationAction(
  _state: ActionState,
  formData: FormData,
): Promise<ActionState> {
  try {
    await api("/api/v1/auth/verification/request", {
      method: "POST",
      body: JSON.stringify({ email: formData.get("email") }),
    });
    return { success: "A fresh verification link has been sent." };
  } catch (error) {
    return { error: message(error) };
  }
}

export async function consentAction(formData: FormData) {
  const requestId = String(formData.get("request_id") ?? "");
  const approve = formData.get("decision") === "approve";
  const [workspaceId, projectId] = String(formData.get("selection") ?? "").split(":");
  const result = await api<{ redirect_to: string }>(
    `/api/v1/oauth/authorization-requests/${encodeURIComponent(requestId)}`,
    {
      method: "POST",
      body: JSON.stringify({
        approve,
        workspace_id: approve ? workspaceId : null,
        project_id: approve ? projectId : null,
      }),
    },
  );
  redirect(result.redirect_to);
}

export async function createWorkspaceAction(
  _state: ActionState,
  formData: FormData,
): Promise<ActionState> {
  try {
    await api("/api/v1/workspaces", {
      method: "POST",
      body: JSON.stringify({ name: formData.get("name") }),
    });
    revalidatePath("/dashboard");
    return { success: "Workspace created." };
  } catch (error) {
    return { error: message(error) };
  }
}

export async function createProjectAction(
  _state: ActionState,
  formData: FormData,
): Promise<ActionState> {
  try {
    await api("/api/v1/projects", {
      method: "POST",
      body: JSON.stringify({
        workspace_id: formData.get("workspace_id"),
        name: formData.get("name"),
      }),
    });
    revalidatePath("/dashboard");
    return { success: "Project created." };
  } catch (error) {
    return { error: message(error) };
  }
}

export async function createApiKeyAction(
  _state: ActionState,
  formData: FormData,
): Promise<ActionState> {
  const projectId = String(formData.get("project_id") ?? "");
  try {
    const result = await api<{ secret: string }>(
      `/api/v1/projects/${encodeURIComponent(projectId)}/api-keys`,
      {
        method: "POST",
        body: JSON.stringify({
          name: formData.get("name"),
          scopes: ["project:read"],
        }),
      },
    );
    revalidatePath("/dashboard");
    return {
      success: "API key created. Copy it now; it will not be shown again.",
      secret: result.secret,
    };
  } catch (error) {
    return { error: message(error) };
  }
}

export async function selectContextAction(formData: FormData) {
  await api("/api/v1/session/selection", {
    method: "POST",
    body: JSON.stringify({
      workspace_id: formData.get("workspace_id"),
      project_id: formData.get("project_id"),
    }),
  });
  revalidatePath("/dashboard");
}

export async function revokeApiKeyAction(formData: FormData) {
  const apiKeyId = String(formData.get("api_key_id") ?? "");
  await api(`/api/v1/api-keys/${encodeURIComponent(apiKeyId)}`, {
    method: "DELETE",
  });
  revalidatePath("/dashboard");
}
