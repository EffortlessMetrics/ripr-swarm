"use strict";

// Behavioral controls for the isolated npm registry and the install-to-use
// journey (#4627). The live journey uses a real RIPR binary when
// RIPR_PACKAGE_TEST_BINARY names one (the Rust-owned npm_consumer test sets
// it to the candidate binary); the fail-closed controls run everywhere.

const assert = require("node:assert/strict");
const fs = require("node:fs");
const http = require("node:http");
const os = require("node:os");
const path = require("node:path");
const zlib = require("node:zlib");
const { spawnSync } = require("node:child_process");
const test = require("node:test");

const { RegistryError, readTarball, startRegistry } = require("../lib/registry.cjs");
const { Journey, buildReceipt, main, npmCliPath, parseArgs, splitCommand, stagedIdentities, which } = require("../lib/journey.cjs");

// Test-only staging: packs the launcher with real `npm pack` and wraps an
// executable in a native package tarball with the same manifest shape the
// xtask producer writes (xtask/src/reports/release_server/npm_native.rs).


const LAUNCHER_ROOT = path.resolve(__dirname, "..", "..", "launcher");

function tarHeader(name, size, mode) {
  const header = Buffer.alloc(512, 0);
  header.write(name, 0, 100, "utf8");
  header.write(`${mode.toString(8).padStart(7, "0")}\0`, 100, 8, "ascii");
  header.write("0000000\0", 108, 8, "ascii");
  header.write("0000000\0", 116, 8, "ascii");
  header.write(`${size.toString(8).padStart(11, "0")}\0`, 124, 12, "ascii");
  header.write("00000000000\0", 136, 12, "ascii");
  header.write("        ", 148, 8, "ascii");
  header.write("0", 156, 1, "ascii");
  header.write("ustar\0", 257, 6, "ascii");
  header.write("00", 263, 2, "ascii");
  let sum = 0;
  for (const byte of header) sum += byte;
  header.write(`${sum.toString(8).padStart(6, "0")}\0 `, 148, 8, "ascii");
  return header;
}

function writeTarball(file, entries) {
  const blocks = [];
  for (const entry of entries) {
    blocks.push(tarHeader(entry.name, entry.bytes.length, entry.mode));
    blocks.push(entry.bytes);
    const pad = (512 - (entry.bytes.length % 512)) % 512;
    if (pad) blocks.push(Buffer.alloc(pad, 0));
  }
  blocks.push(Buffer.alloc(1024, 0));
  fs.writeFileSync(file, zlib.gzipSync(Buffer.concat(blocks)));
}

function launcherManifest() {
  return JSON.parse(fs.readFileSync(path.join(LAUNCHER_ROOT, "package.json"), "utf8"));
}

function currentPlatform() {
  const libc = process.platform === "linux" ? "glibc" : null;
  return launcherManifest().ripr.platforms.find((entry) =>
    entry.nodePlatform === process.platform && entry.nodeArch === process.arch && entry.libc === libc);
}

function packLauncher(destination) {
  const result = spawnSync(process.execPath, [npmCliPath(), "pack", "--ignore-scripts", "--json", "--pack-destination", destination, LAUNCHER_ROOT], {
    encoding: "utf8",
    env: { ...process.env, npm_config_cache: path.join(destination, ".pack-cache"), npm_config_update_notifier: "false" },
  });
  if (result.status !== 0) throw new Error(`npm pack failed: ${result.stderr}`);
  fs.rmSync(path.join(destination, ".pack-cache"), { recursive: true, force: true });
  return path.join(destination, JSON.parse(result.stdout)[0].filename);
}

// Wraps `executableBytes` as the native package for `platform`.
function packNative(destination, platform, version, executableBytes, overrides = {}) {
  const manifest = {
    name: platform.package,
    version,
    description: "Native ripr executable (journey test staging)",
    license: "MIT OR Apache-2.0",
    os: [platform.nodePlatform],
    cpu: [platform.nodeArch],
    files: [platform.executable],
    exports: { "./package.json": "./package.json" },
    riprNative: { schemaVersion: 1, product: "ripr", target: platform.rustTarget, executable: platform.executable },
    ...overrides,
  };
  if (platform.libc) manifest.libc = [platform.libc];
  const file = path.join(destination, `${platform.package.slice(1).replace("/", "-")}-${version}.tgz`);
  writeTarball(file, [
    { name: "package/package.json", bytes: Buffer.from(`${JSON.stringify(manifest, null, 2)}\n`), mode: 0o644 },
    { name: `package/${platform.executable}`, bytes: executableBytes, mode: 0o755 },
  ]);
  return file;
}

const createdDirs = [];
function tempDir(label) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), `ripr-npm-${label}-`));
  createdDirs.push(dir);
  return dir;
}
// Staged tarballs (including a packaged candidate executable) and receipts
// are test-owned; remove them when the suite ends.
test.after(() => {
  for (const dir of createdDirs) fs.rmSync(dir, { recursive: true, force: true });
});


const VERSION = launcherManifest().version;
const POSIX = process.platform !== "win32";

function get(url, method = "GET") {
  return new Promise((resolve, reject) => {
    const request = http.request(url, { method }, (response) => {
      const chunks = [];
      response.on("data", (chunk) => chunks.push(chunk));
      response.on("end", () => resolve({ status: response.statusCode, body: Buffer.concat(chunks) }));
    });
    request.on("error", reject);
    request.end();
  });
}

function stageFamily(label, executableBytes) {
  const dir = tempDir(label);
  packLauncher(dir);
  const platform = currentPlatform();
  if (platform && executableBytes) packNative(dir, platform, VERSION, executableBytes);
  // Foreign natives carry a placeholder: the journey must never fetch them.
  for (const other of launcherManifest().ripr.platforms) {
    if (!platform || other.package !== platform.package) packNative(dir, other, VERSION, Buffer.from("foreign placeholder\n"));
  }
  return { dir, platform };
}

async function runJourney(packages, extra = []) {
  const out = tempDir("receipt");
  const receipt = path.join(out, "receipt.json");
  const code = await main(["--packages", packages, "--target", currentPlatform().rustTarget, "--version", VERSION,
    "--receipt", receipt, "--log", path.join(out, "log.json"),
    "--source-commit", "a".repeat(40), "--source-tree", "b".repeat(40), ...extra]);
  return { code, receipt: JSON.parse(fs.readFileSync(receipt, "utf8")), log: JSON.parse(fs.readFileSync(path.join(out, "log.json"), "utf8")), out };
}

function failedChecks(log) {
  return log.checks.filter((item) => item.status !== "passed").map((item) => item.name);
}

test("isolated registry serves only staged tarballs, read-only, on loopback", async () => {
  const { dir } = stageFamily("registry", Buffer.from("not a real executable"));
  const tarballs = fs.readdirSync(dir).map((name) => path.join(dir, name));
  const registry = await startRegistry(tarballs);
  try {
    assert.match(registry.origin, /^http:\/\/127\.0\.0\.1:\d+$/);
    const packument = await get(`${registry.origin}/@effortlessmetrics%2fripr`);
    assert.equal(packument.status, 200);
    const document = JSON.parse(packument.body.toString("utf8"));
    const launcher = registry.staged.find((item) => item.manifest.name === "@effortlessmetrics/ripr");
    assert.equal(document.versions[VERSION].dist.integrity, launcher.integrity);
    assert.equal(document.versions[VERSION].scripts, undefined, "packuments must not advertise scripts");
    const tarball = await get(document.versions[VERSION].dist.tarball);
    assert.equal(tarball.status, 200);
    assert.ok(tarball.body.equals(launcher.bytes), "served tarball bytes must equal the staged bytes");
    assert.equal((await get(`${registry.origin}/left-pad`)).status, 404, "no upstream fallback");
    assert.equal((await get(`${registry.origin}/-/tarballs/left-pad-1.0.0.tgz`)).status, 404);
    assert.equal((await get(`${registry.origin}/@effortlessmetrics%2fripr`, "PUT")).status, 405, "no publish route");
    assert.equal((await get(`${registry.origin}/%E0%A4%A`)).status, 400, "a malformed path must not crash the registry");
    assert.equal((await get(`${registry.origin}/@effortlessmetrics%2fripr`)).status, 200, "the registry still serves after a malformed path");
    assert.deepEqual(registry.requests.map((entry) => entry.status), [200, 200, 404, 404, 405, 400, 200]);
  } finally {
    await registry.close();
  }
  await assert.rejects(get(`${registry.origin}/@effortlessmetrics%2fripr`), "a closed registry must refuse connections");
});

test("isolated registry refuses install-time lifecycle scripts and duplicate staging", async () => {
  const dir = tempDir("lifecycle");
  const platform = currentPlatform() || launcherManifest().ripr.platforms[0];
  const scripted = packNative(dir, platform, VERSION, Buffer.from("x"), { scripts: { postinstall: "node fetch.js" } });
  await assert.rejects(startRegistry([scripted]), (error) => error instanceof RegistryError && error.code === "tarball_lifecycle_script");
  const uninstallHook = packNative(tempDir("uninstall-hook"), platform, VERSION, Buffer.from("x"), { scripts: { preuninstall: "node cleanup.js" } });
  await assert.rejects(startRegistry([uninstallHook]), (error) => error instanceof RegistryError && error.code === "tarball_lifecycle_script");
  const clean = packNative(tempDir("dup"), platform, VERSION, Buffer.from("x"));
  await assert.rejects(startRegistry([clean, clean]), (error) => error instanceof RegistryError && error.code === "registry_duplicate");
  await assert.rejects(startRegistry([]), (error) => error instanceof RegistryError && error.code === "registry_empty");
  assert.throws(() => readTarball(Buffer.from("plain")), (error) => error.code === "tarball_invalid");
});

test("staged identities refuse range pins, missing targets and mismatched native metadata", () => {
  const manifest = launcherManifest();
  const platform = manifest.ripr.platforms[0];
  const nativeEntry = { name: `package/${platform.executable}`, mode: 0o755, bytes: Buffer.from("payload") };
  const native = (overrides = {}) => ({
    basename: "native.tgz", entries: [nativeEntry], sha256: "sha256:x",
    manifest: { name: platform.package, version: VERSION, riprNative: { target: platform.rustTarget, executable: platform.executable }, ...overrides },
  });
  const launcher = (edit = (value) => value) => ({ basename: "launcher.tgz", entries: [], manifest: edit(JSON.parse(JSON.stringify(manifest))) });
  const options = { version: VERSION, target: platform.rustTarget };
  const foreign = manifest.ripr.platforms.slice(1).map((entry) => ({
    basename: `${entry.rustTarget}.tgz`, entries: [], sha256: "sha256:y",
    manifest: { name: entry.package, version: VERSION, riprNative: { target: entry.rustTarget, executable: entry.executable } },
  }));
  const ok = stagedIdentities({ staged: [launcher(), native(), ...foreign] }, options);
  assert.equal(ok.platform.package, platform.package);
  assert.match(ok.payloadHash, /^sha256:[0-9a-f]{64}$/);
  assert.throws(() => stagedIdentities({ staged: [launcher((value) => {
    value.optionalDependencies[platform.package] = `^${VERSION}`;
    return value;
  }), native()] }, options), /must pin .* to exactly/);
  assert.throws(() => stagedIdentities({ staged: [launcher()] }, options), /no staged .* tarball for/);
  assert.throws(() => stagedIdentities({ staged: [launcher(), native({ riprNative: { target: "x86_64-unknown-linux-musl", executable: platform.executable } })] }, options), /riprNative metadata must name/);
  assert.throws(() => stagedIdentities({ staged: [launcher(), native({ version: "0.0.1" })] }, options), /no staged/);
  assert.throws(() => stagedIdentities({ staged: [launcher((value) => ({ ...value, dependencies: { leftpad: "1.0.0" } })), native()] }, options), /runtime dependencies/);
  assert.throws(() => stagedIdentities({ staged: [launcher(), native(), ...foreign.slice(1)] }, options), /every optional native the launcher pins/);
  const base = ["--packages", "p", "--target", "t", "--version", "v", "--receipt", "r"];
  assert.throws(() => parseArgs([...base, "--source-commit", "HEAD", "--source-tree", "b".repeat(40)]), /40-character/);
  assert.throws(() => parseArgs(base), /--source-commit is required/);
  assert.throws(() => parseArgs([...base, "--source-commit", "a".repeat(40), "--source-tree", "b".repeat(40), "--bogus", "x"]), /unknown argument/);
  assert.equal(parseArgs([...base, "--keep", "--source-commit", "a".repeat(40), "--source-tree", "b".repeat(40)]).keep, true);
});

test("receipt rows fail closed on any failed check and never pass without an executed payload", () => {
  const options = { target: "x86_64-unknown-linux-gnu", version: VERSION, sourceCommit: "a".repeat(40), sourceTree: "b".repeat(40) };
  const journey = { npm: npmCliPath(), root: os.tmpdir(), env: () => process.env, lockfileHash: null };
  const identities = {
    launcher: { sha256: `sha256:${"1".repeat(64)}` },
    native: { sha256: `sha256:${"2".repeat(64)}`, manifest: { name: "n", version: VERSION } },
    payloadHash: `sha256:${"3".repeat(64)}`,
  };
  const passedPayload = { hash: identities.payloadHash };
  journey.checks = [{ name: "a", status: "passed" }, { name: "b", status: "failed", detail: "boom" }];
  const failed = buildReceipt(options, journey, { identities, payload: passedPayload, timings: {} }, "passed");
  assert.equal(failed.rows[0].status, "failed");
  assert.equal(failed.rows[0].subject_count, 0);
  assert.equal(failed.subjects.failed, 1);
  assert.match(failed.rows[0].limitations[0], /^b: boom/);
  journey.checks = [{ name: "a", status: "passed" }];
  assert.equal(buildReceipt(options, journey, { identities, payload: undefined, timings: {} }, "passed").rows[0].status, "failed",
    "a row with no executed payload cannot pass");
  const passed = buildReceipt(options, journey, { identities, payload: passedPayload, timings: {} }, "passed");
  assert.equal(passed.rows[0].status, "passed");
  assert.equal(passed.rows[0].executed_payload_hash, identities.payloadHash);
  assert.deepEqual(passed.non_claims, ["not a public registry publication", "not a support-tier promotion", "not a runtime mutation result", "not authorization to publish"]);
});

test("journey fails closed when the target native package is not staged", async (t) => {
  if (!currentPlatform()) {
    t.skip(`no launcher platform for ${process.platform}-${process.arch}`);
    return;
  }
  const { dir } = stageFamily("no-native", null);
  const { code, receipt, log } = await runJourney(dir);
  assert.equal(code, 1);
  assert.equal(receipt.rows[0].status, "failed");
  assert.ok(failedChecks(log).includes("isolated_registry_started"), JSON.stringify(failedChecks(log)));
});

test("journey fails closed when a substituted native prints the right version but is not ripr", async (t) => {
  if (!POSIX || !currentPlatform()) {
    t.skip("substituted-executable control uses a POSIX shell payload");
    return;
  }
  const impostor = Buffer.from(`#!/bin/sh\necho 'ripr ${VERSION} (impostor)'\nexit 0\n`);
  const { dir } = stageFamily("impostor", impostor);
  const { code, receipt, log } = await runJourney(dir);
  assert.equal(code, 1);
  assert.equal(receipt.rows[0].status, "failed");
  assert.equal(receipt.rows[0].subject_count, 0);
  const failures = failedChecks(log);
  assert.ok(failures.includes("typescript_check_nonzero_subject"), JSON.stringify(failures));
  // The install, identity and version steps pass: version text alone is not proof.
  for (const name of ["registry_install_scripts_disabled", "installed_payload_identity", "project_script_version_offline"]) {
    assert.ok(!failures.includes(name), `${name} should pass for the impostor: ${JSON.stringify(failures)}`);
  }
});

test("live journey installs the real ripr payload through the isolated registry", async (t) => {
  const binary = process.env.RIPR_PACKAGE_TEST_BINARY;
  if (!binary || !currentPlatform()) {
    t.skip("set RIPR_PACKAGE_TEST_BINARY to an absolute ripr binary path");
    return;
  }
  assert.ok(path.isAbsolute(binary), "RIPR_PACKAGE_TEST_BINARY must be absolute");
  const bytes = fs.readFileSync(binary);
  const { dir } = stageFamily("live", bytes);
  const { code, receipt, log } = await runJourney(dir);
  assert.deepEqual(failedChecks(log), []);
  assert.equal(code, 0);
  const row = receipt.rows[0];
  assert.equal(row.status, "passed");
  assert.equal(row.executed_payload_hash, `sha256:${require("node:crypto").createHash("sha256").update(bytes).digest("hex")}`);
  assert.ok(row.subject_count >= 18, `expected the full journey, got ${row.subject_count} checks`);
  assert.equal(receipt.cleanup.status, "passed");
  const checkNames = log.checks.map((item) => item.name);
  for (const required of ["typescript_check_nonzero_subject", "continuation_new_process_foreign_cwd_offline",
    "control_missing_optional_package", "control_wrong_native_version", "control_foreign_target_native",
    "reinstall_from_lockfile_offline", "registry_teardown", "tool_identities"]) {
    assert.ok(checkNames.includes(required), `${required} must run`);
  }
});

test("an explicit work directory must be new or empty and is never reused", async () => {
  const work = tempDir("work");
  fs.writeFileSync(path.join(work, "keep-me.txt"), "caller data");
  const out = tempDir("work-receipt");
  await assert.rejects(main(["--packages", work, "--target", "x86_64-unknown-linux-gnu", "--version", VERSION,
    "--receipt", path.join(out, "r.json"), "--source-commit", "a".repeat(40), "--source-tree", "b".repeat(40), "--work", work]),
  /must be a new or empty directory/);
  assert.equal(fs.readFileSync(path.join(work, "keep-me.txt"), "utf8"), "caller data");
});

test("tarball reader refuses duplicate entry names", () => {
  const dir = tempDir("dup-entry");
  const file = path.join(dir, "dup.tgz");
  writeTarball(file, [
    { name: "package/package.json", bytes: Buffer.from('{"name":"a","version":"1.0.0"}'), mode: 0o644 },
    { name: "package/bin/ripr", bytes: Buffer.from("one"), mode: 0o755 },
    { name: "package/bin/ripr", bytes: Buffer.from("two"), mode: 0o755 },
  ]);
  assert.throws(() => readTarball(fs.readFileSync(file)), /appears more than once/);
});

test("continuation commands split exactly as ripr quotes them on both hosts", () => {
  assert.deepEqual(splitCommand("ripr explain --root '/tmp/a b ü' --worktree probe:x"),
    ["ripr", "explain", "--root", "/tmp/a b ü", "--worktree", "probe:x"]);
  assert.deepEqual(splitCommand("ripr x 'it'\\''s'"), ["ripr", "x", "it's"]);
  assert.deepEqual(splitCommand('ripr explain --root "C:\\Users\\a b\\\\" --worktree p'),
    ["ripr", "explain", "--root", "C:\\Users\\a b\\", "--worktree", "p"]);
  assert.deepEqual(splitCommand('ripr x "say ""hi"""'), ["ripr", "x", 'say "hi"']);
  assert.throws(() => splitCommand("ripr x \"$(printf a)\""), /shell expansion/);
  assert.throws(() => splitCommand("ripr x 'open"), /unterminated/);
});

test("the consumer PATH keeps git but not a toolchain that shares its directory", (t) => {
  if (!POSIX) {
    t.skip("the linked tools directory is POSIX");
    return;
  }
  // Homebrew-style layout: git, rustup, cargo and rustc in one directory.
  const shared = tempDir("shared-bin");
  for (const name of ["git", "rustup", "cargo", "rustc"]) {
    fs.writeFileSync(path.join(shared, name), "#!/bin/sh\nexit 0\n", { mode: 0o755 });
  }
  const saved = process.env.PATH;
  const journey = new Journey({});
  try {
    process.env.PATH = `${shared}:${saved}`;
    const scrubbed = journey.scrubbedPath();
    for (const tool of ["rustup", "cargo", "rustc"]) {
      assert.equal(which(tool, scrubbed), null, `${tool} leaked into ${scrubbed}`);
    }
    assert.equal(fs.realpathSync(which("git", scrubbed)), fs.realpathSync(path.join(shared, "git")));
    assert.ok(which("node", scrubbed), "node must stay reachable for npm scripts");
  } finally {
    process.env.PATH = saved;
    journey.cleanup();
  }
});

test("a step timeout takes down the whole process group", (t) => {
  if (!POSIX) {
    t.skip("process-group teardown is POSIX");
    return;
  }
  const journey = new Journey({ stepTimeoutMs: 500 });
  try {
    journey.childPath = "/usr/bin:/bin";
    const result = journey.run("hang", "/bin/sh", ["-c", "sleep 30 & echo $!; wait"], journey.root);
    assert.equal(result.error && result.error.code, "ETIMEDOUT");
    const grandchild = Number.parseInt(String(result.stdout).trim(), 10);
    assert.ok(grandchild > 0, `expected a grandchild pid, got ${JSON.stringify(result.stdout)}`);
    let alive = true;
    for (let attempt = 0; attempt < 50 && alive; attempt += 1) {
      try {
        process.kill(grandchild, 0);
        spawnSync("/bin/sleep", ["0.1"]);
      } catch {
        alive = false;
      }
    }
    assert.equal(alive, false, "the timed-out step left a descendant running");
  } finally {
    journey.cleanup();
  }
});

test("cleanup reaps a descendant that outlived a step that exited normally", (t) => {
  if (!POSIX) {
    t.skip("process-group reaping is POSIX");
    return;
  }
  const journey = new Journey({});
  journey.childPath = "/usr/bin:/bin";
  const result = journey.run("leaves a child", "/bin/sh", ["-c", "sleep 30 >/dev/null 2>&1 & echo $!"], journey.root);
  assert.equal(result.status, 0);
  const grandchild = Number.parseInt(String(result.stdout).trim(), 10);
  assert.ok(grandchild > 0, `expected a grandchild pid, got ${JSON.stringify(result.stdout)}`);
  assert.doesNotThrow(() => process.kill(grandchild, 0), "the grandchild should still be running before cleanup");
  assert.equal(journey.cleanup(), "passed");
  let alive = true;
  for (let attempt = 0; attempt < 50 && alive; attempt += 1) {
    try {
      process.kill(grandchild, 0);
      spawnSync("/bin/sleep", ["0.1"]);
    } catch {
      alive = false;
    }
  }
  assert.equal(alive, false, "cleanup left the step's descendant running");
});

test("a Windows step timeout fails cleanup closed because descendant teardown is not owned there", () => {
  const journey = new Journey({ stepTimeoutMs: 300 });
  journey.platform = "win32";
  const result = journey.run("hang", process.execPath, ["-e", "setTimeout(() => {}, 30000)"], journey.root);
  assert.equal(result.error && result.error.code, "ETIMEDOUT");
  assert.equal(journey.cleanup(), "failed");
  const quick = new Journey({ stepTimeoutMs: 30000 });
  quick.platform = "win32";
  assert.equal(quick.run("quick", process.execPath, ["-e", "0"], quick.root).status, 0);
  assert.equal(quick.cleanup(), "passed");
});

test("journey fails closed when the emitted continuation is unusable", async (t) => {
  if (!POSIX || !currentPlatform()) {
    t.skip("continuation control uses a POSIX shell payload");
    return;
  }
  // Behaves like ripr for --version and check, but prints a continuation
  // with a flag its own explain refuses. Replaying a reconstructed command
  // would hide that; replaying the emitted one must not.
  const report = JSON.stringify({
    summary: { changed_files_by_language: [{ language: "typescript", files: 1 }] },
    findings: [{ id: "probe:x", classification: "weakly_exposed", probe: { file: "src/pricing.ts" } }],
    finding_routes: [{ finding_id: "probe:x", explain_command: "ripr explain --invalid-option probe:x" }],
  });
  const script = [
    "#!/bin/sh",
    `case "$1" in`,
    `  --version) echo 'ripr ${VERSION} (impostor)' ;;`,
    `  check) printf '%s\\n' '${report}' ;;`,
    `  explain) case "$2" in --invalid-option) echo 'unknown option' >&2; exit 2 ;; *) echo 'src/pricing.ts' ;; esac ;;`,
    "esac",
    "",
  ].join("\n");
  const { dir } = stageFamily("bad-continuation", Buffer.from(script));
  const { code, receipt, log } = await runJourney(dir);
  assert.equal(code, 1);
  assert.equal(receipt.rows[0].status, "failed");
  const failures = failedChecks(log);
  assert.deepEqual(failures, ["continuation_new_process_foreign_cwd_offline"], JSON.stringify(failures));
});

test("the entrypoint reports its code through ordinary teardown, not process.exit", () => {
  // process.exit() from the entrypoint deadlocked on aarch64-apple-darwin
  // after a passed journey (main thread joining a V8 worker that awaited a
  // main-thread collection). A preload records any process.exit call and,
  // for the settled-journey rows, replaces lib main with a fixed outcome so
  // the passed, failed and rejected exits are each exercised.
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "ripr-journey-exit-"));
  try {
    const marker = path.join(dir, "exit-called");
    const lib = path.resolve(__dirname, "..", "lib", "journey.cjs");
    const rows = [
      { name: "passed journey", main: "async () => 0", status: 0, stderr: /^$/ },
      { name: "failed journey", main: "async () => 1", status: 1, stderr: /^$/ },
      { name: "rejected journey", main: "async () => { throw new Error('boom'); }", status: 2, stderr: /ripr npm journey: boom/ },
      { name: "real usage failure", main: null, status: 2, stderr: /--packages is required/ },
    ];
    for (const row of rows) {
      const preload = path.join(dir, "preload.cjs");
      fs.writeFileSync(preload, [
        "const fs = require('node:fs');",
        "const exit = process.exit;",
        `process.exit = (code) => { fs.writeFileSync(${JSON.stringify(marker)}, String(code)); exit(code); };`,
        row.main ? `require.cache[${JSON.stringify(lib)}] = { id: ${JSON.stringify(lib)}, filename: ${JSON.stringify(lib)}, loaded: true, exports: { main: ${row.main} } };` : "",
        "",
      ].join("\n"));
      fs.rmSync(marker, { force: true });
      const result = spawnSync(process.execPath, ["--require", preload, path.resolve(__dirname, "..", "bin", "journey.cjs")], {
        encoding: "utf8",
        timeout: 8000,
      });
      assert.equal(result.error, undefined, row.name);
      assert.equal(result.status, row.status, row.name);
      assert.match(result.stderr, row.stderr, row.name);
      assert.equal(fs.existsSync(marker), false, `${row.name}: the entrypoint called process.exit`);
    }
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
