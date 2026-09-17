#!/usr/bin/env bash
set -Eeuo pipefail

usage() {
  cat <<'EOF'
Usage: ./deploy/jio.sh [vm-id]

Deploy the current Git branch natively to a Jio Large VM (4 vCPU, 8 GiB).
Without vm-id, the last deployment VM is reused or a new one is created.
Progress goes to stderr; stdout contains only the final landing URL.

Environment:
  JIO_BIN      Jio CLI path (default: jio)
  JIO_VM_FILE  Remembered VM path (default: .local/jio-vm)
  GIT_REPO     Repository URL visible from the VM (default: origin)
  GIT_REF      Branch, tag, or commit to deploy (default: current branch)
EOF
}

die() {
  echo "error: $*" >&2
  exit 1
}

base64_one_line() {
  base64 | tr -d '\r\n'
}

case "${1:-}" in
  -h|--help)
    usage
    exit 0
    ;;
esac
(( $# <= 1 )) || die "expected at most one VM ID"

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
jio_bin="$(command -v "${JIO_BIN:-jio}")" || die "jio CLI not found"
command -v curl >/dev/null || die "curl is required"
command -v git >/dev/null || die "git is required"

git_repo="${GIT_REPO:-$(git -C "$project_root" remote get-url origin)}"
git_ref="${GIT_REF:-$(git -C "$project_root" branch --show-current)}"
git_ref="${git_ref:-main}"
case "$git_repo" in
  git@github.com:*) git_repo="https://github.com/${git_repo#git@github.com:}" ;;
esac

vm_file="${JIO_VM_FILE:-$project_root/.local/jio-vm}"
vm_id="${1:-}"
remembered_vm=false
if [[ -z "$vm_id" && -f "$vm_file" ]]; then
  IFS= read -r vm_id < "$vm_file" || true
  remembered_vm=true
fi

created_vm=false
vm_line="$("$jio_bin" list | awk -v id="$vm_id" '$1 == id { print; exit }')"
if [[ -n "$vm_id" && -z "$vm_line" ]]; then
  echo "Jio VM $vm_id no longer exists; creating a replacement..." >&2
  vm_id=""
fi
if [[ -z "$vm_id" ]]; then
  echo "Creating Jio Large VM..." >&2
  vm_id="$("$jio_bin" create)"
  created_vm=true
  echo "Created $vm_id" >&2
  vm_line="$("$jio_bin" list | awk -v id="$vm_id" '$1 == id { print; exit }')"
elif [[ "$remembered_vm" == true ]]; then
  echo "Reusing Jio VM $vm_id..." >&2
fi

[[ -n "$vm_line" ]] || die "VM $vm_id was not found"
if [[ "$vm_line" != *"Large · 4 vCPU · 8 GiB"* ]]; then
  if [[ "$created_vm" == true ]]; then
    "$jio_bin" destroy "$vm_id" --yes >/dev/null 2>&1 || true
  fi
  die "VM $vm_id is not a Large 4-vCPU VM; run 'jio config' and choose Large"
fi
mkdir -p "$(dirname "$vm_file")"
printf '%s\n' "$vm_id" > "$vm_file"

JIO_BIN="$jio_bin" bash "$project_root/deploy/ensure-ready.sh" "$vm_id"

echo "Resolving Jio endpoints..." >&2
published_ports="$("$jio_bin" ports "$vm_id")"
resolve_endpoint() {
  local port="$1" url
  url="$(printf '%s\n' "$published_ports" | awk -v port="$port" '$1 == port && $2 == "published" { print $3; exit }')"
  if [[ -z "$url" ]]; then
    url="$("$jio_bin" expose "$port" "$vm_id")"
  fi
  printf '%s\n' "$url"
}
app_url="$(resolve_endpoint 8080)"
landing_url="$(resolve_endpoint 3002)"
docs_url="$(resolve_endpoint 3003)"
for url in "$app_url" "$landing_url" "$docs_url"; do
  [[ "$url" == https://* ]] || die "Jio returned an invalid public URL: $url"
done

nginx_b64="$(base64_one_line <<'NGINX'
user www-data;
worker_processes auto;
pid /run/nginx.pid;
error_log /var/log/nginx/error.log;

events {
  worker_connections 1024;
}

http {
  log_format safe '$remote_addr - $request_method $uri $status $body_bytes_sent';
  access_log /var/log/nginx/access.log safe;

  server {
    listen 127.0.0.1:8080;
    server_name _;

    proxy_http_version 1.1;
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-Host $host;
    proxy_set_header X-Forwarded-Proto https;
    proxy_set_header Connection "";
    proxy_buffering off;
    proxy_read_timeout 3600s;

    location = /oauth/consent { proxy_pass http://127.0.0.1:3000; }
    location = /mcp { proxy_pass http://127.0.0.1:4001; }
    location = /.well-known/oauth-protected-resource { proxy_pass http://127.0.0.1:4001; }
    location = /.well-known/oauth-protected-resource/mcp { proxy_pass http://127.0.0.1:4001; }
    location /api/ { proxy_pass http://127.0.0.1:4000; }
    location /oauth/ { proxy_pass http://127.0.0.1:4000; }
    location /.well-known/ { proxy_pass http://127.0.0.1:4000; }
    location / { proxy_pass http://127.0.0.1:3000; }
  }
}
NGINX
)"

repo_b64="$(printf '%s' "$git_repo" | base64_one_line)"
ref_b64="$(printf '%s' "$git_ref" | base64_one_line)"
app_b64="$(printf '%s' "$app_url" | base64_one_line)"
landing_b64="$(printf '%s' "$landing_url" | base64_one_line)"
docs_b64="$(printf '%s' "$docs_url" | base64_one_line)"
read -r -d '' remote_script <<'REMOTE' || true
set -Eeuo pipefail

decode() { printf '%s' "$1" | base64 -d; }
wait_http() {
  local service="$1" url="$2"
  for _ in $(seq 1 60); do
    curl -fsS "$url" >/dev/null 2>&1 && return 0
    sleep 1
  done
  echo "$service did not become ready" >&2
  sudo -n journalctl -u "$service" --no-pager -n 100 >&2 || true
  return 1
}
write_unit() {
  sudo -n tee "/etc/systemd/system/$1.service" >/dev/null
}

repo_url="$(decode '__REPO_B64__')"
git_ref="$(decode '__REF_B64__')"
app_url="$(decode '__APP_B64__')"
landing_url="$(decode '__LANDING_B64__')"
docs_url="$(decode '__DOCS_B64__')"
state_dir=/var/lib/saas-template
repo_dir=$state_dir/repo
cargo_home=$state_dir/cargo
rustup_home=$state_dir/rustup
target_dir=$state_dir/target
secrets_dir=$state_dir/secrets
deployed_revision_file=$state_dir/deployed-revision
deploy_user="$(id -un)"
deploy_group="$(id -gn)"

command -v git >/dev/null
command -v curl >/dev/null
command -v openssl >/dev/null
sudo -n true
sudo -n install -d -m 0755 -o "$deploy_user" -g "$deploy_group" \
  "$state_dir" "$cargo_home" "$rustup_home" "$target_dir"
sudo -n install -d -m 0700 -o "$deploy_user" -g "$deploy_group" "$secrets_dir"

if [[ ! -d "$repo_dir/.git" ]]; then
  [[ ! -e "$repo_dir" ]] || { echo "$repo_dir exists but is not a Git checkout" >&2; exit 1; }
  git clone --depth 1 --no-checkout "$repo_url" "$repo_dir"
fi
previous_revision=""
if [[ -s "$deployed_revision_file" ]]; then
  IFS= read -r previous_revision < "$deployed_revision_file"
elif sudo -n systemctl is-active --quiet saas-api 2>/dev/null; then
  previous_revision="$(git -C "$repo_dir" rev-parse HEAD 2>/dev/null || true)"
fi
if [[ -n "$previous_revision" ]] && git -C "$repo_dir" cat-file -e "$previous_revision^{commit}" 2>/dev/null; then
  full_deploy=false
else
  previous_revision=""
  full_deploy=true
fi
git -C "$repo_dir" remote set-url origin "$repo_url"
git -C "$repo_dir" fetch --depth 1 origin "$git_ref"
git -C "$repo_dir" checkout --force --detach FETCH_HEAD
revision="$(git -C "$repo_dir" rev-parse HEAD)"

changed() {
  [[ "$full_deploy" == true ]] ||
    ! git -C "$repo_dir" diff --quiet "$previous_revision" "$revision" -- "$@"
}

api_changed=false
mcp_changed=false
app_changed=false
landing_changed=false
docs_changed=false
runtime_changed=false
changed Cargo.toml Cargo.lock rust-toolchain rust-toolchain.toml .cargo crates/auth crates/db services/api && api_changed=true
changed Cargo.toml Cargo.lock rust-toolchain rust-toolchain.toml .cargo crates/auth crates/mcp services/mcp && mcp_changed=true
changed package.json bun.lock apps/app && app_changed=true
changed package.json bun.lock apps/landing && landing_changed=true
changed package.json bun.lock apps/docs docs && docs_changed=true
changed deploy && runtime_changed=true
[[ -x "$target_dir/release/starter-api" ]] || api_changed=true
[[ -x "$target_dir/release/starter-mcp-server" ]] || mcp_changed=true
[[ -f "$repo_dir/apps/app/.next/BUILD_ID" ]] || app_changed=true
[[ -f "$repo_dir/apps/landing/.next/BUILD_ID" ]] || landing_changed=true
[[ -f "$repo_dir/apps/docs/.next/BUILD_ID" ]] || docs_changed=true
printf 'Deploy plan: api=%s mcp=%s app=%s landing=%s docs=%s runtime=%s\n' \
  "$api_changed" "$mcp_changed" "$app_changed" "$landing_changed" "$docs_changed" "$runtime_changed"

umask 077
if [[ -f "$state_dir/postgres.env" ]]; then
  db_secret="$(sed -n 's/^POSTGRES_PASSWORD=//p' "$state_dir/postgres.env")"
else
  db_secret="$(openssl rand -hex 24)"
fi
[[ "$db_secret" =~ ^[0-9a-f]{48}$ ]] || { echo "invalid stored database password" >&2; exit 1; }
printf 'POSTGRES_USER=saas\nPOSTGRES_PASSWORD=%s\nPOSTGRES_DB=saas_template\n' "$db_secret" > "$state_dir/postgres.env"

private_key=$secrets_dir/auth-private.pem
public_key=$secrets_dir/auth-public.pem
if [[ ! -e "$private_key" && ! -e "$public_key" ]]; then
  openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out "$private_key" >/dev/null 2>&1
  openssl pkey -in "$private_key" -pubout -out "$public_key" >/dev/null 2>&1
elif [[ ! -f "$private_key" || ! -f "$public_key" ]]; then
  echo "signing key pair is incomplete; refusing to rotate it" >&2
  exit 1
fi

printf 'APP_ENV=production\nAPP_NAME=Agent SaaS Starter\nAPP_URL=%s\nLANDING_URL=%s\nDOCS_URL=%s\nAPI_PUBLIC_URL=%s\nAPI_INTERNAL_URL=http://127.0.0.1:4000\nAUTH_ISSUER=%s\nMCP_PUBLIC_URL=%s\nMCP_RESOURCE=%s/mcp\nDATABASE_URL=postgres://saas:%s@127.0.0.1:5432/saas_template\nAPI_BIND=127.0.0.1:4000\nMCP_BIND=127.0.0.1:4001\nAUTH_PRIVATE_KEY_PATH=%s\nAUTH_PUBLIC_KEY_PATH=%s\nAUTH_KEY_ID=jio-%s\nSESSION_TTL_SECONDS=2592000\nACCESS_TOKEN_TTL_SECONDS=900\nREFRESH_TOKEN_TTL_SECONDS=2592000\nAUTH_CODE_TTL_SECONDS=300\nAUTH_REQUEST_TTL_SECONDS=600\nHANDOFF_TTL_SECONDS=120\nOIDC_PROVIDERS_JSON={}\nDCR_ENABLED=true\nCIMD_ALLOWED_ORIGINS=https://chatgpt.com,https://claude.ai\nRUST_LOG=starter_api=info,starter_mcp_server=info,tower_http=info\n' \
  "$app_url" "$landing_url" "$docs_url" "$app_url" "$app_url" "$app_url" "$app_url" "$db_secret" \
  "$private_key" "$public_key" '__VM_ID__' > "$state_dir/saas.env"
chmod 600 "$state_dir/saas.env" "$state_dir/postgres.env" "$private_key"
chmod 644 "$public_key"

if ! command -v cc >/dev/null || ! command -v pkg-config >/dev/null || [[ ! -x /usr/sbin/nginx ]] || ! dpkg-query -W libssl-dev >/dev/null 2>&1; then
  echo "Installing native build dependencies and nginx..."
  sudo -n apt-get update -qq
  sudo -n env DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    build-essential ca-certificates curl libssl-dev nginx pkg-config
fi

export CARGO_HOME="$cargo_home"
export RUSTUP_HOME="$rustup_home"
export CARGO_TARGET_DIR="$target_dir"
export PATH="$cargo_home/bin:$PATH"
if [[ ! -x "$cargo_home/bin/rustup" ]]; then
  echo "Installing Rust 1.94 toolchain..."
  curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs \
    | sh -s -- -y --no-modify-path --profile minimal --default-toolchain 1.94.0
else
  rustup toolchain list | grep -q '^1\.94\.0' || rustup toolchain install 1.94.0 --profile minimal
  rustup default 1.94.0
fi

rust_packages=()
if [[ "$api_changed" == true ]]; then rust_packages+=(-p starter-api); fi
if [[ "$mcp_changed" == true ]]; then rust_packages+=(-p starter-mcp-server); fi
if (( ${#rust_packages[@]} )); then
  echo "Building changed Rust services..."
  cargo build --locked --release "${rust_packages[@]}" --manifest-path "$repo_dir/Cargo.toml"
else
  echo "Rust services unchanged; skipping build."
fi

export APP_URL="$app_url"
export LANDING_URL="$landing_url"
export DOCS_URL="$docs_url"
export API_PUBLIC_URL="$app_url"
export API_INTERNAL_URL=http://127.0.0.1:4000
export NEXT_TELEMETRY_DISABLED=1
touch "$repo_dir/.env"
cd "$repo_dir"
if [[ "$app_changed" == true || "$landing_changed" == true || "$docs_changed" == true ]]; then
  echo "Building changed web apps..."
  bun install --frozen-lockfile
  if [[ "$app_changed" == true ]]; then bun run build:app; fi
  if [[ "$landing_changed" == true ]]; then bun run build:landing; fi
  if [[ "$docs_changed" == true ]]; then bun run build:docs; fi
else
  echo "Web apps unchanged; skipping build."
fi

if ! command -v psql >/dev/null; then
  echo "Installing PostgreSQL..."
  sudo -n apt-get update -qq
  sudo -n env DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends postgresql
fi
sudo -n systemctl enable --now postgresql >/dev/null
for _ in $(seq 1 60); do
  pg_isready -h 127.0.0.1 >/dev/null 2>&1 && break
  sleep 1
done
pg_isready -h 127.0.0.1 >/dev/null

sudo -n -u postgres psql -v ON_ERROR_STOP=1 <<SQL
DO \$\$ BEGIN
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'saas') THEN
    CREATE ROLE saas LOGIN PASSWORD '$db_secret';
  END IF;
END \$\$;
ALTER ROLE saas WITH LOGIN PASSWORD '$db_secret';
SELECT 'CREATE DATABASE saas_template OWNER saas'
WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'saas_template')\gexec
ALTER DATABASE saas_template OWNER TO saas;
SQL

decode '__NGINX_B64__' | sudo -n tee /etc/nginx/nginx.conf >/dev/null
sudo -n /usr/sbin/nginx -t

write_unit saas-api <<EOF
[Unit]
After=network-online.target postgresql.service
Wants=network-online.target
Requires=postgresql.service

[Service]
User=$deploy_user
Group=$deploy_group
WorkingDirectory=$repo_dir
EnvironmentFile=$state_dir/saas.env
ExecStart=$target_dir/release/starter-api
Restart=on-failure
RestartSec=2

[Install]
WantedBy=multi-user.target
EOF

write_unit saas-mcp <<EOF
[Unit]
After=network-online.target saas-api.service
Wants=network-online.target

[Service]
User=$deploy_user
Group=$deploy_group
WorkingDirectory=$repo_dir
EnvironmentFile=$state_dir/saas.env
ExecStart=$target_dir/release/starter-mcp-server
Restart=on-failure
RestartSec=2

[Install]
WantedBy=multi-user.target
EOF

for web_service in "app 3000" "landing 3002" "docs 3003"; do
  read -r name port <<< "$web_service"
  write_unit "saas-$name" <<EOF
[Unit]
After=network-online.target
Wants=network-online.target

[Service]
User=$deploy_user
Group=$deploy_group
WorkingDirectory=$repo_dir/apps/$name
EnvironmentFile=$state_dir/saas.env
Environment=NODE_ENV=production
ExecStart=/usr/local/bin/node $repo_dir/apps/$name/node_modules/next/dist/bin/next start --hostname 127.0.0.1 --port $port
Restart=on-failure
RestartSec=2

[Install]
WantedBy=multi-user.target
EOF
done

sudo -n systemctl daemon-reload
sudo -n systemctl enable saas-api saas-mcp saas-app saas-landing saas-docs nginx >/dev/null

if [[ "$api_changed" == true || "$runtime_changed" == true ]]; then
  echo "Restarting API and applying embedded database migrations..."
  sudo -n systemctl restart saas-api
  wait_http saas-api http://127.0.0.1:4000/readyz
fi
if [[ "$mcp_changed" == true || "$runtime_changed" == true ]]; then sudo -n systemctl restart saas-mcp; fi
if [[ "$app_changed" == true || "$runtime_changed" == true ]]; then sudo -n systemctl restart saas-app; fi
if [[ "$landing_changed" == true || "$runtime_changed" == true ]]; then sudo -n systemctl restart saas-landing; fi
if [[ "$docs_changed" == true || "$runtime_changed" == true ]]; then sudo -n systemctl restart saas-docs; fi
if [[ "$runtime_changed" == true ]]; then sudo -n systemctl restart nginx; fi
wait_http saas-api http://127.0.0.1:4000/readyz
wait_http saas-mcp http://127.0.0.1:4001/healthz
wait_http saas-app http://127.0.0.1:3000/login
wait_http saas-landing http://127.0.0.1:3002/
wait_http saas-docs http://127.0.0.1:3003/
wait_http nginx http://127.0.0.1:8080/login

printf '%s\n' "$revision" > "$deployed_revision_file"
echo "Deployed ${revision:0:12}"
sudo -n systemctl --no-pager --plain --type=service --state=running \
  | awk '$1 ~ /^(nginx|postgresql|saas-)/ { print $1 }'
REMOTE

remote_script="${remote_script//__REPO_B64__/$repo_b64}"
remote_script="${remote_script//__REF_B64__/$ref_b64}"
remote_script="${remote_script//__APP_B64__/$app_b64}"
remote_script="${remote_script//__LANDING_B64__/$landing_b64}"
remote_script="${remote_script//__DOCS_B64__/$docs_b64}"
remote_script="${remote_script//__NGINX_B64__/$nginx_b64}"
remote_script="${remote_script//__VM_ID__/${vm_id:0:12}}"

echo "Deploying $git_repo ($git_ref) to $vm_id..." >&2
printf '%s\n' "$remote_script" | "$jio_bin" connect "$vm_id" >&2

wait_public() {
  local name="$1" url="$2" code
  for _ in $(seq 1 30); do
    code="$(curl -sS -o /dev/null -w '%{http_code}' "$url" 2>/dev/null || true)"
    [[ "$code" == 200 ]] && return 0
    sleep 2
  done
  die "$name failed its public smoke check ($url)"
}

wait_public app "$app_url/login"
wait_public landing "$landing_url/"
wait_public docs "$docs_url/"
wait_public oauth "$app_url/.well-known/oauth-authorization-server"
wait_public mcp "$app_url/.well-known/oauth-protected-resource"

oauth_document="$(curl -fsS "$app_url/.well-known/oauth-authorization-server")"
mcp_document="$(curl -fsS "$app_url/.well-known/oauth-protected-resource")"
[[ "$oauth_document" == *"\"issuer\":\"$app_url\""* ]] || die "OAuth issuer does not match the app URL"
[[ "$mcp_document" == *"\"resource\":\"$app_url/mcp\""* ]] || die "MCP resource does not match the app URL"
[[ "$(curl -sS -o /dev/null -w '%{http_code}' -X POST "$app_url/mcp")" == 401 ]] || die "MCP endpoint is not protected"

{
  echo
  echo "VM:      $vm_id (Large · 4 vCPU · 8 GiB)"
  echo "Landing: ${landing_url%/}/"
  echo "App:     ${app_url%/}/"
  echo "Docs:    ${docs_url%/}/"
} >&2
printf '%s/\n' "${landing_url%/}"
