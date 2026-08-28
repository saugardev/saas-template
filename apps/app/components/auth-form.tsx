"use client";

import Link from "next/link";
import { useActionState, useState } from "react";
import type { ActionState } from "@/app/actions";
import { SubmitButton } from "@/components/submit-button";

type AuthFormProps = {
  mode: "login" | "register";
  action: (state: ActionState, data: FormData) => Promise<ActionState>;
  returnTo?: string;
};

export function AuthForm({
  mode,
  action,
  returnTo = "/dashboard",
}: AuthFormProps) {
  const [state, formAction] = useActionState(action, {});
  const [showPassword, setShowPassword] = useState(false);
  const registering = mode === "register";

  return (
    <form action={formAction} className="space-y-5">
      <input type="hidden" name="return_to" value={returnTo} />
      {state.error ? (
        <p
          role="alert"
          className="rounded-lg bg-destructive/10 px-4 py-3 text-sm text-destructive"
        >
          {state.error}
        </p>
      ) : null}
      {registering ? (
        <Field label="Name" id="name">
          <input
            className="field"
            id="name"
            name="name"
            autoComplete="name"
            minLength={2}
            required
          />
        </Field>
      ) : null}
      <Field label="Email" id="email">
        <input
          className="field"
          id="email"
          name="email"
          type="email"
          autoComplete="email"
          spellCheck={false}
          placeholder="you@example.com"
          required
        />
      </Field>
      <Field label="Password" id="password">
        <div className="relative">
          <input
            className="field pr-20"
            id="password"
            name="password"
            type={showPassword ? "text" : "password"}
            autoComplete={registering ? "new-password" : "current-password"}
            minLength={8}
            required
          />
          <button
            className="absolute inset-y-0 right-1 my-auto min-h-10 rounded-md px-3 text-sm font-medium text-muted-foreground hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
            type="button"
            onClick={() => setShowPassword((visible) => !visible)}
            aria-label={showPassword ? "Hide password" : "Show password"}
          >
            {showPassword ? "Hide" : "Show"}
          </button>
        </div>
        {registering ? (
          <p className="mt-1.5 text-xs text-muted-foreground">
            Use at least 8 characters.
          </p>
        ) : null}
      </Field>
      <SubmitButton>
        {registering ? "Create account" : "Sign in"}
      </SubmitButton>
      <div className="flex items-center justify-between gap-4 text-sm text-muted-foreground">
        <Link className="link" href={registering ? "/login" : "/register"}>
          {registering ? "Already have an account?" : "Create an account"}
        </Link>
        {!registering ? (
          <Link className="link" href="/forgot-password">
            Forgot password?
          </Link>
        ) : null}
      </div>
    </form>
  );
}

function Field({
  label,
  id,
  children,
}: {
  label: string;
  id: string;
  children: React.ReactNode;
}) {
  return (
    <div>
      <label className="mb-1.5 block text-sm font-medium" htmlFor={id}>
        {label}
      </label>
      {children}
    </div>
  );
}

