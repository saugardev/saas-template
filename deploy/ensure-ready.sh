#!/usr/bin/env bash
set -Eeuo pipefail

vm_id="${1:?VM ID is required}"
jio_bin="${JIO_BIN:-jio}"
if result="$("$jio_bin" exec "$vm_id" true --timeout 15 2>&1)"; then
  exit 0
fi

# Jio currently has no machine-readable CLI state command. Only a confirmed
# stopped state warrants a start; authentication and SSH failures must surface.
if [[ "$result" != "jio: session $vm_id is Stopped" ]]; then
  printf '%s\n' "$result" >&2
  exit 1
fi

echo "Starting stopped Jio VM $vm_id..." >&2
"$jio_bin" start "$vm_id" >/dev/null
"$jio_bin" exec "$vm_id" true --timeout 15 >/dev/null
