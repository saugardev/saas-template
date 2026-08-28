"use client";

import { useActionState } from "react";
import type { ActionState } from "@/app/actions";
import { SubmitButton } from "@/components/submit-button";

export function SimpleForm({
  action,
  children,
  submitLabel,
}: {
  action: (state: ActionState, data: FormData) => Promise<ActionState>;
  children: React.ReactNode;
  submitLabel: string;
}) {
  const [state, formAction] = useActionState(action, {});
  return (
    <form action={formAction} className="space-y-4">
      {children}
      {state.error ? (
        <p role="alert" className="text-sm text-destructive">
          {state.error}
        </p>
      ) : null}
      {state.success ? (
        <p role="status" className="text-sm text-success">
          {state.success}
        </p>
      ) : null}
      {state.secret ? (
        <div className="rounded-lg bg-muted p-3">
          <p className="mb-1 text-xs font-medium text-muted-foreground">
            New secret
          </p>
          <code className="break-all font-mono text-sm">{state.secret}</code>
        </div>
      ) : null}
      <SubmitButton>{submitLabel}</SubmitButton>
    </form>
  );
}

