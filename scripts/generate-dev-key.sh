#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
key_dir="$project_root/.local"
private_key="$key_dir/auth-private.pem"
public_key="$key_dir/auth-public.pem"

mkdir -p "$key_dir"

if [[ -e "$private_key" || -e "$public_key" ]]; then
  echo "Refusing to overwrite an existing key. Remove the files in .local explicitly to rotate them."
  exit 1
fi

openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out "$private_key"
openssl pkey -in "$private_key" -pubout -out "$public_key"
chmod 600 "$private_key"
chmod 644 "$public_key"

echo "Generated development signing keys in .local/."

