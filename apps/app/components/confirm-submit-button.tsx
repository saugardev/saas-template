"use client";

import { useState } from "react";
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
  const [confirming, setConfirming] = useState(false);

  return (
    <span className="inline-flex items-center justify-end gap-1">
      {confirming ? <span className="sr-only" role="status">{confirmMessage}</span> : null}
      <button
        type={confirming ? "submit" : "button"}
        disabled={pending}
        aria-busy={pending}
        onClick={() => setConfirming(true)}
        className={className}
      >
        {pending ? "Revoking…" : confirming ? "Confirm revoke" : children}
      </button>
      {confirming && !pending ? (
        <button type="button" className="button-table" onClick={() => setConfirming(false)}>Cancel</button>
      ) : null}
    </span>
  );
}
