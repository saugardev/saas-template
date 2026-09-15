#!/usr/bin/env bash
set -Eeuo pipefail

usage() {
  cat <<'EOF'
Usage: ./deploy/jio.sh [vm-id]

Deploy the current Git branch to a Jio Large VM (4 vCPU, 8 GiB).
Without vm-id, the last deployment VM is reused or a new one is created.
Progress goes to stderr; stdout contains only the final landing URL.

Environment:
  JIO_BIN      Jio CLI path (default: jio)
  JIO_VM_FILE  Remembered VM path (default: .local/jio-vm)
  GIT_REPO     Repository URL visible from the VM (default: origin)
  GIT_REF      Branch or tag to deploy (default: current branch)
  DEPLOY_IMAGE Prebuilt image to pull instead of building on the VM
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

echo "Publishing Jio endpoints..." >&2
app_url="$("$jio_bin" expose 8080 "$vm_id")"
landing_url="$("$jio_bin" expose 3002 "$vm_id")"
docs_url="$("$jio_bin" expose 3003 "$vm_id")"
for url in "$app_url" "$landing_url" "$docs_url"; do
  [[ "$url" == https://* ]] || die "Jio returned an invalid public URL: $url"
done

nginx_b64="$(base64_one_line <<'NGINX'
events {}
http {
  log_format safe '$remote_addr - $request_method $uri $status $body_bytes_sent';
  access_log /dev/stdout safe;
  error_log /dev/stderr warn;

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
image_b64="$(printf '%s' "${DEPLOY_IMAGE:-}" | base64_one_line)"

read -r -d '' remote_script <<'REMOTE' || true
set -Eeuo pipefail

decode() { printf '%s' "$1" | base64 -d; }
docker() { sudo -n docker "$@"; }
wait_http() {
  local name="$1" url="$2"
  for _ in $(seq 1 60); do
    curl -fsS "$url" >/dev/null 2>&1 && return 0
    sleep 1
  done
  echo "$name did not become ready" >&2
  docker logs --tail 100 "$name" >&2 || true
  return 1
}

repo_url="$(decode '__REPO_B64__')"
git_ref="$(decode '__REF_B64__')"
app_url="$(decode '__APP_B64__')"
landing_url="$(decode '__LANDING_B64__')"
docs_url="$(decode '__DOCS_B64__')"
prebuilt_image="$(decode '__IMAGE_B64__')"
repo_dir=/workspace/saas-template

command -v git >/dev/null
command -v curl >/dev/null
command -v openssl >/dev/null
docker info >/dev/null

if [[ -d "$repo_dir/.git" ]]; then
  [[ -z "$(git -C "$repo_dir" status --porcelain --untracked-files=no)" ]] || {
    echo "tracked changes exist in $repo_dir; refusing to overwrite them" >&2
    exit 1
  }
  git -C "$repo_dir" remote set-url origin "$repo_url"
  git -C "$repo_dir" fetch --depth 1 origin "$git_ref"
  git -C "$repo_dir" checkout --detach FETCH_HEAD
else
  git clone --depth 1 --branch "$git_ref" "$repo_url" "$repo_dir"
fi

decode '__NGINX_B64__' > /workspace/saas-nginx.conf

umask 077
if [[ -f /workspace/postgres.env ]]; then
  db_secret="$(sed -n 's/^POSTGRES_PASSWORD=//p' /workspace/postgres.env)"
else
  db_secret="$(openssl rand -hex 24)"
fi
[[ "$db_secret" =~ ^[0-9a-f]{48}$ ]] || {
  echo "invalid stored database password" >&2
  exit 1
}
printf 'POSTGRES_USER=saas\nPOSTGRES_PASSWORD=%s\nPOSTGRES_DB=saas_template\n' "$db_secret" > /workspace/postgres.env
printf 'APP_ENV=production\nAPP_NAME=Agent SaaS Starter\nAPP_URL=%s\nLANDING_URL=%s\nDOCS_URL=%s\nAPI_PUBLIC_URL=%s\nAPI_INTERNAL_URL=http://127.0.0.1:4000\nAUTH_ISSUER=%s\nMCP_PUBLIC_URL=%s\nMCP_RESOURCE=%s/mcp\nDATABASE_URL=postgres://saas:%s@127.0.0.1:5432/saas_template\nAPI_BIND=127.0.0.1:4000\nMCP_BIND=127.0.0.1:4001\nAUTH_PRIVATE_KEY_PATH=/run/secrets/auth-private.pem\nAUTH_PUBLIC_KEY_PATH=/run/secrets/auth-public.pem\nAUTH_KEY_ID=jio-%s\nSESSION_TTL_SECONDS=2592000\nACCESS_TOKEN_TTL_SECONDS=900\nREFRESH_TOKEN_TTL_SECONDS=2592000\nAUTH_CODE_TTL_SECONDS=300\nAUTH_REQUEST_TTL_SECONDS=600\nHANDOFF_TTL_SECONDS=120\nOIDC_PROVIDERS_JSON={}\nDCR_ENABLED=true\nCIMD_ALLOWED_ORIGINS=https://chatgpt.com,https://claude.ai\nRUST_LOG=starter_api=info,starter_mcp_server=info,tower_http=info\n' \
  "$app_url" "$landing_url" "$docs_url" "$app_url" "$app_url" "$app_url" "$app_url" "$db_secret" '__VM_ID__' > /workspace/saas.env

mkdir -p /workspace/saas-secrets
private_key=/workspace/saas-secrets/auth-private.pem
public_key=/workspace/saas-secrets/auth-public.pem
if [[ ! -e "$private_key" && ! -e "$public_key" ]]; then
  openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out "$private_key" >/dev/null 2>&1
  openssl pkey -in "$private_key" -pubout -out "$public_key" >/dev/null 2>&1
elif [[ ! -f "$private_key" || ! -f "$public_key" ]]; then
  echo "signing key pair is incomplete; refusing to rotate it" >&2
  exit 1
fi
chmod 600 /workspace/saas.env /workspace/postgres.env "$private_key"
chmod 644 /workspace/saas-nginx.conf "$public_key"

revision="$(git -C "$repo_dir" rev-parse --short=12 HEAD)"
if [[ -n "$prebuilt_image" ]]; then
  image="$prebuilt_image"
  docker pull "$image"
else
  image="saas-template:$revision"
  docker build --progress=plain -f "$repo_dir/deploy/Dockerfile" -t "$image" \
    --build-arg "APP_URL=$app_url" \
    --build-arg "LANDING_URL=$landing_url" \
    --build-arg "DOCS_URL=$docs_url" \
    --build-arg "API_PUBLIC_URL=$app_url" \
    "$repo_dir"
fi

for container in saas-proxy saas-docs saas-landing saas-app saas-mcp saas-api saas-db; do
  docker rm -f "$container" >/dev/null 2>&1 || true
done

docker volume create saas-postgres >/dev/null
docker run -d --name saas-db --restart unless-stopped --network host \
  --env-file /workspace/postgres.env \
  -v saas-postgres:/var/lib/postgresql/data \
  postgres:16-alpine postgres -c listen_addresses=127.0.0.1 >/dev/null
for _ in $(seq 1 60); do
  docker exec saas-db pg_isready -U saas -d saas_template >/dev/null 2>&1 && break
  sleep 1
done
docker exec saas-db pg_isready -U saas -d saas_template >/dev/null

docker run -d --name saas-api --restart unless-stopped --network host \
  --env-file /workspace/saas.env \
  -v /workspace/saas-secrets:/run/secrets:ro \
  --entrypoint /usr/local/bin/starter-api "$image" >/dev/null
wait_http saas-api http://127.0.0.1:4000/readyz

docker run -d --name saas-mcp --restart unless-stopped --network host \
  --env-file /workspace/saas.env \
  -v /workspace/saas-secrets:/run/secrets:ro \
  --entrypoint /usr/local/bin/starter-mcp-server "$image" >/dev/null
docker run -d --name saas-app --restart unless-stopped --network host \
  --env-file /workspace/saas.env --workdir /srv/apps/app --entrypoint node \
  "$image" node_modules/next/dist/bin/next start --hostname 127.0.0.1 --port 3000 >/dev/null
docker run -d --name saas-landing --restart unless-stopped --network host \
  --env-file /workspace/saas.env --workdir /srv/apps/landing --entrypoint node \
  "$image" node_modules/next/dist/bin/next start --hostname 127.0.0.1 --port 3002 >/dev/null
docker run -d --name saas-docs --restart unless-stopped --network host \
  --env-file /workspace/saas.env --workdir /srv/apps/docs --entrypoint node \
  "$image" node_modules/next/dist/bin/next start --hostname 127.0.0.1 --port 3003 >/dev/null

wait_http saas-mcp http://127.0.0.1:4001/healthz
wait_http saas-app http://127.0.0.1:3000/login
wait_http saas-landing http://127.0.0.1:3002/
wait_http saas-docs http://127.0.0.1:3003/

docker run -d --name saas-proxy --restart unless-stopped --network host \
  -v /workspace/saas-nginx.conf:/etc/nginx/nginx.conf:ro \
  nginx:1.27-alpine >/dev/null
wait_http saas-proxy http://127.0.0.1:8080/login

echo "Deployed $revision"
docker ps --format 'table {{.Names}}\t{{.Status}}'
REMOTE

remote_script="${remote_script//__REPO_B64__/$repo_b64}"
remote_script="${remote_script//__REF_B64__/$ref_b64}"
remote_script="${remote_script//__APP_B64__/$app_b64}"
remote_script="${remote_script//__LANDING_B64__/$landing_b64}"
remote_script="${remote_script//__DOCS_B64__/$docs_b64}"
remote_script="${remote_script//__IMAGE_B64__/$image_b64}"
remote_script="${remote_script//__NGINX_B64__/$nginx_b64}"
remote_script="${remote_script//__VM_ID__/${vm_id:0:12}}"
remote_b64="$(printf '%s' "$remote_script" | base64_one_line)"

echo "Deploying $git_repo ($git_ref) to $vm_id..." >&2
"$jio_bin" exec "$vm_id" "printf '%s' '$remote_b64' | base64 -d | bash" --timeout 3600 >&2

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
