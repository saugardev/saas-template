"use client";

import { useFormStatus } from "react-dom";

export function SubmitButton({
  children,
  className = "",
}: {
  children: React.ReactNode;
  className?: string;
}) {
  const { pending } = useFormStatus();
  return (
    <button
      type="submit"
      disabled={pending}
      aria-busy={pending}
      className={`button-primary min-h-11 w-full disabled:opacity-55 ${className}`}
    >
      {pending ? "Working…" : children}
    </button>
  );
}
