import { expect, test } from "bun:test";
import { chmodSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

// Run the actual workflow shell and released CLI; only the remote API, SSH and
// tag lookup are faked. No production credentials or machines are used.
const root = resolve(import.meta.dir, "..");
const workflow = Bun.YAML.parse(readFileSync(join(root, ".github/workflows/deploy-jio.yml"), "utf8")) as {
  jobs: { deploy: { steps: { name?: string; run?: string }[] } };
};
const resolveRuntime = workflow.jobs.deploy.steps.find(step => step.name === "Resolve production runtime")!.run!;
const vm = "a".repeat(32);
const hostKey = (byte: number) => "ssh-ed25519 " + Buffer.concat([
  Buffer.from("0000000b7373682d6564323535313900000020", "hex"), Buffer.alloc(32, byte),
]).toString("base64");

test("deployment resumes stopped VMs and refreshes stale pins without hiding failures", async () => {
  const jio = Bun.which("jio");
  if (!jio) throw new Error("Install the released Jio CLI before running test:deploy");

  for (const scenario of ["stopped", "ready", "api-error", "ssh-error", "start-error", "ssh-after-start-error"]) {
    const directory = mkdtempSync(join(tmpdir(), "jio-deploy-test-"));
    let starts = 0;
    const session = {
      session_id: vm, state: scenario === "ready" || scenario === "ssh-error" ? "ready" : "stopped",
      generation: 2, size: "large", vcpu_count: 4, memory_mib: 8192,
      template_id: "b".repeat(64), core_sha256: "c".repeat(64), volume_id: "d".repeat(32),
      workspace_path: "/workspace", guest_ipv4: "172.31.0.2", ssh_port: 22, ssh_username: "jio",
      ssh_host_public_key: hostKey(2), guest_ready_ns: 1, storage_ready_ns: 2,
      network_ready_ns: 3, ssh_ready_ns: 4, worker_ready_ns: 5, session_ready_ns: 6,
      storage_attach_ns: 1, worker_spawn_ns: 1, worker_template_prepare_ns: 1, cow_fork_ns: 1,
      vm_create_ns: 1, state_restore_ns: 1, device_restore_ns: 1, vsock_transport_reset_ns: 1,
      vsock_connect_ns: 1, vsock_init_ns: 1, access_probe_ns: 1,
    };
    const server = Bun.serve({
      hostname: "127.0.0.1", port: 0,
      fetch(request) {
        expect(request.headers.get("authorization")).toBe(`Bearer ${"e".repeat(64)}`);
        const path = new URL(request.url).pathname;
        if (request.method === "GET" && path === "/v1/sessions") {
          return Response.json({ sessions: [{ id: vm, state: session.state, release: { vcpu: 4, memory_mib: 8192 } }], next_after: null });
        }
        if (request.method === "GET" && path === `/v0/sessions/${vm}`) {
          return scenario === "api-error"
            ? Response.json({ error: "invalid API key" }, { status: 401 })
            : Response.json(session);
        }
        if (request.method === "POST" && path === `/v0/sessions/${vm}/start`) {
          starts++;
          expect(session.state).toBe("stopped");
          expect(request.headers.get("if-match")).toBe("2");
          if (scenario === "start-error") return Response.json({ error: "no capacity" }, { status: 409 });
          session.state = "ready";
          session.generation++;
          session.ssh_host_public_key = hostKey(3);
          return Response.json(session);
        }
        throw new Error(`Unexpected request: ${request.method} ${path}`);
      },
    });
    try {
      symlinkSync(jio, join(directory, "jio"));
      const key = join(directory, "deploy-key");
      const keygen = Bun.spawnSync(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-C", "", "-f", key]);
      expect(keygen.exitCode).toBe(0);
      writeFileSync(join(directory, "git"), `#!/usr/bin/env bash
case "$1" in
  fetch) exit 0 ;;
  tag) printf '%s\\n%s\\n' "$TEST_VM" "$TEST_OLD_HOST_KEY" ;;
  *) exit 99 ;;
esac
`);
      writeFileSync(join(directory, "ssh"), `#!/usr/bin/env bash
set -eu
[[ " $* " == *" StrictHostKeyChecking=yes "* ]]
[[ "$(cat "$JIO_STATE_DIR/sessions/$TEST_VM/known_hosts")" == "jio-$TEST_VM $TEST_EXPECTED_HOST_KEY" ]]
if [[ "$TEST_SCENARIO" == *ssh*error ]]; then echo 'SSH unavailable' >&2; exit 255; fi
`);
      for (const binary of ["git", "ssh"]) chmodSync(join(directory, binary), 0o700);
      const output = join(directory, "output");
      const process = Bun.spawn(["bash", "-c", resolveRuntime], {
        cwd: root, stdout: "pipe", stderr: "pipe",
        env: { ...Bun.env, PATH: `${directory}:${Bun.env.PATH}`,
          JIO_API_KEY: "e".repeat(64), JIO_ENDPOINT: server.url.origin,
          JIO_SSH_KEY: readFileSync(key, "utf8"), JIO_STATE_DIR: join(directory, "state"),
          RUNNER_TEMP: directory, GITHUB_OUTPUT: output, CONFIGURED_VM_ID: vm,
          CONFIGURED_HOST_KEY: hostKey(1), TEST_VM: vm, TEST_OLD_HOST_KEY: hostKey(1),
          TEST_EXPECTED_HOST_KEY: hostKey(session.state === "ready" ? 2 : 3), TEST_SCENARIO: scenario,
        },
      });
      const [code, stderr] = await Promise.all([process.exited, new Response(process.stderr).text()]);
      const success = scenario === "stopped" || scenario === "ready";
      expect({ scenario, code, error: success ? stderr.replace(/Starting stopped Jio VM .*\n/, "") : "" }).toEqual({ scenario, code: success ? 0 : 1, error: "" });
      expect(starts).toBe(["stopped", "start-error", "ssh-after-start-error"].includes(scenario) ? 1 : 0);
      if (success) expect(readFileSync(output, "utf8")).toBe(`id=${vm}\nhost_key=${session.ssh_host_public_key}\n`);
      else expect(stderr).not.toBe("");
      expect(readFileSync(join(directory, "state", "sessions", vm, "id_ed25519"), "utf8").trim()).toBe(readFileSync(key, "utf8").trim());
    } finally {
      server.stop(true);
      rmSync(directory, { recursive: true, force: true });
    }
  }
}, 30000);
