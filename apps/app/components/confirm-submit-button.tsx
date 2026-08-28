"use client";

import { useFormStatus } from "react-dom";

export function ConfirmSubmitButton({
  children,
  confirmMessage,
  className = "",
}: {
  children: React.ReactNode;
  confirmMessage: string;
  className?: string;
}) {
  const { pending } = useFormStatus();

  return (
    <button
      type="submit"
      disabled={pending}
      aria-busy={pending}
      onClick={(event) => {
        if (!window.confirm(confirmMessage)) event.preventDefault();
      }}
      className={className}
    >
      {pending ? "Revoking…" : children}
    </button>
  );
}
