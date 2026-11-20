"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const os = require("node:os");
const crypto = require("node:crypto");
const { spawnSync } = require("node:child_process");
const test = require("node:test");

const PROTOCOL_VERSION = "2025-11-25";
const MAX_RESPONSE_BYTES = 128 * 1024;
const FIXTURE_GAP = "gap:rust:src/lib.rs:discounted_total:predicate_boundary:predicate:amount==threshold";

function digest(file, algorithm = "sha256", encoding = "hex") {
  return crypto.createHash(algorithm).update(fs.readFileSync(file)).digest(encoding);
}

function mcpReplies(stdout) {
  const text = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(stdout);
  const lines = text.split("\n");
  assert.equal(lines.pop(), "", "stdout must end at a protocol frame boundary");
  assert.equal(lines.length, 4, "expected only initialize, tools/list, refresh and gap list responses");
  return lines.map((line, index) => {
    assert.ok(line.startsWith("{") && line.endsWith("}") && !line.includes("\r"),
      "stdout must contain only LF-delimited JSON objects");
    assert.ok(Buffer.byteLength(line) <= MAX_RESPONSE_BYTES, "MCP response exceeded the byte limit");
    const reply = JSON.parse(line);
    assert.equal(reply.jsonrpc, "2.0");
    assert.equal(reply.id, index + 1);
    assert.equal(reply.error, undefined, line);
    assert.ok(reply.result, line);
    return reply;
  });
}

function analysisResult(results, version) {
  const [initialize, discovery, refreshResult, listResult] = results;
  assert.equal(initialize.protocolVersion, PROTOCOL_VERSION);
  assert.equal(initialize.serverInfo.name, "ripr");
  assert.equal(initialize.serverInfo.version, version);
  for (const name of ["ripr_refresh", "ripr_list_gaps"]) {
    assert.ok(discovery.tools.some((tool) => tool.name === name), `tools/list must advertise ${name}`);
  }
  assert.notEqual(refreshResult.isError, true, JSON.stringify(refreshResult));
  assert.notEqual(listResult.isError, true, JSON.stringify(listResult));
  const refresh = refreshResult.structuredContent;
  const list = listResult.structuredContent;
  assert.equal(refresh.schema_version, "ripr-mcp-refresh-v1");
  assert.deepEqual(refresh.attempt, { state: "completed", failure: null });
  assert.equal(refresh.scope.analyzed_base, "main");
  assert.ok(refresh.snapshot.finding_count > 0, "refresh must analyze a nonempty diff");
  assert.match(refresh.snapshot.snapshot_id, /^snapshot:sha256:[a-f0-9]{64}$/);
  assert.equal(list.schema_version, "ripr-mcp-gap-list-v5");
  assert.equal(list.snapshot_id, refresh.snapshot.snapshot_id);
  assert.ok(list.items.some((item) => item.language === "rust" && item.file === "./src/lib.rs" &&
    item.canonical_id === FIXTURE_GAP), "gap list must contain the fixture's changed Rust function");
  return list;
}

function createAnalysisFixture(workspace) {
  fs.mkdirSync(path.join(workspace, "src"), { recursive: true });
  fs.writeFileSync(path.join(workspace, ".gitignore"), "/target/\n");
  fs.writeFileSync(path.join(workspace, "Cargo.toml"), [
    "[package]",
    'name = "npm-mcp-smoke"',
    'version = "0.0.0"',
    'edition = "2024"',
    "[workspace]",
    "",
  ].join("\n"));
  const git = (...args) => {
    const result = spawnSync("git", [
      "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgSign=false", "-c", "core.autocrlf=false",
      "-c", "user.name=RIPR fixture", "-c", "user.email=fixture@ripr.invalid", ...args,
    ], { cwd: workspace, encoding: "utf8", timeout: 15000 });
    assert.equal(result.status, 0, `git ${args.join(" ")}: ${result.error || result.stderr}`);
  };
  const source = path.join(workspace, "src", "lib.rs");
  const writeBoundary = (operator) => fs.writeFileSync(source, [
    "pub fn discounted_total(amount: i32, threshold: i32) -> i32 {",
    `    if amount ${operator} threshold {`,
    "        amount - 10",
    "    } else {",
    "        amount",
    "    }",
    "}",
    "",
  ].join("\n"));
  git("init", "-q", "-b", "main");
  writeBoundary(">=");
  git("add", ".");
  git("commit", "-qm", "Open discount boundary");
  git("checkout", "-qb", "analysis");
  writeBoundary(">");
  git("commit", "-qam", "Close discount boundary");
}

// Opt in with an absolute path to a built RIPR binary. This stages a local
// test payload; it does not qualify the release producer or a published package.
test("installed npm launcher preserves a real MCP exchange and EOF", (t) => {
  const binary = process.env.RIPR_PACKAGE_TEST_BINARY;
  if (!binary || process.platform !== "linux" || process.arch !== "x64") {
    t.skip("requires RIPR_PACKAGE_TEST_BINARY and Linux x64 glibc");
    return;
  }
  assert.ok(path.isAbsolute(binary), "binary must be an absolute path");
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "ripr-installed-mcp-"));
  try {
    const launcherRoot = path.join(__dirname, "..");
    const version = JSON.parse(fs.readFileSync(path.join(launcherRoot, "package.json"), "utf8")).version;
    const native = path.join(root, "native");
    const project = path.join(root, "project");
    const foreign = path.join(root, "foreign");
    const workspace = path.join(root, "analysis-fixture");
    const cache = path.join(root, "npm-cache");
    const npm = process.env.npm_execpath ? process.execPath : "npm";
    const npmPrefix = process.env.npm_execpath ? [process.env.npm_execpath] : [];
    const runNpm = (args, cwd) => {
      const result = spawnSync(npm, [...npmPrefix, ...args], {
        cwd, encoding: "utf8", timeout: 60000,
        env: { ...process.env, npm_config_cache: cache, npm_config_offline: "true" },
      });
      assert.equal(result.status, 0, result.stderr || String(result.error));
      return result.stdout;
    };
    fs.mkdirSync(path.join(native, "bin"), { recursive: true });
    fs.mkdirSync(project);
    fs.mkdirSync(foreign);
    createAnalysisFixture(workspace);
    const nativeBinary = path.join(native, "bin", "ripr");
    fs.copyFileSync(binary, nativeBinary);
    fs.chmodSync(nativeBinary, 0o755);
    fs.writeFileSync(path.join(native, "package.json"), JSON.stringify({
      name: "@effortlessmetrics/ripr-linux-x64-gnu", version,
      riprNative: { schemaVersion: 1, product: "ripr", target: "x86_64-unknown-linux-gnu", executable: "bin/ripr" },
    }));
    fs.writeFileSync(path.join(project, "package.json"), '{"private":true}');
    const nativePack = JSON.parse(runNpm(["pack", "--json", "--ignore-scripts", "--pack-destination", root], native))[0];
    const launcherPack = JSON.parse(runNpm(["pack", "--json", "--ignore-scripts", "--pack-destination", root], launcherRoot))[0];
    const archives = [nativePack, launcherPack].map((pack) => {
      const archive = path.join(root, pack.filename);
      assert.equal(pack.version, version);
      assert.equal(`sha512-${digest(archive, "sha512", "base64")}`, pack.integrity);
      return archive;
    });
    assert.deepEqual([nativePack.name, launcherPack.name], ["@effortlessmetrics/ripr-linux-x64-gnu", "@effortlessmetrics/ripr"]);
    runNpm(["install", "--offline", "--ignore-scripts", "--no-audit", "--no-fund", "--package-lock=true", ...archives], project);
    const lock = JSON.parse(fs.readFileSync(path.join(project, "package-lock.json"), "utf8"));
    for (const pack of [nativePack, launcherPack]) {
      const installedPackage = lock.packages[`node_modules/${pack.name}`];
      assert.equal(installedPackage.resolved, `file:../${pack.filename}`);
      assert.equal(installedPackage.integrity, pack.integrity);
      assert.equal(installedPackage.version, version);
    }
    const installed = path.join(project, "node_modules", "@effortlessmetrics", "ripr-linux-x64-gnu", "bin", "ripr");
    const binarySha256 = digest(binary);
    assert.equal(digest(installed), binarySha256, "installed executable must equal selected binary");
    const request = [
      { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: PROTOCOL_VERSION, capabilities: {}, clientInfo: { name: "package-test", version: "1" } } },
      { jsonrpc: "2.0", method: "notifications/initialized" },
      { jsonrpc: "2.0", id: 2, method: "tools/list", params: {} },
      { jsonrpc: "2.0", id: 3, method: "tools/call", params: { name: "ripr_refresh", arguments: { source: "committed", base: "main" } } },
      { jsonrpc: "2.0", id: 4, method: "tools/call", params: { name: "ripr_list_gaps", arguments: { limit: 10 } } },
    ].map((value) => JSON.stringify(value)).join("\n") + "\n";
    // The transport flushes each admitted reply before reading the next frame,
    // so the piped requests reach analysis and the final EOF in order.
    const args = ["mcp", "--stdio", "--root", workspace];
    const options = { cwd: foreign, input: request, timeout: 15000, maxBuffer: 1024 * 1024 };
    const launcherPackage = path.join(project, "node_modules", "@effortlessmetrics", "ripr");
    for (const file of ["bin/ripr.cjs", "lib/launcher.cjs"]) {
      assert.equal(digest(path.join(launcherPackage, file)), digest(path.join(launcherRoot, file)));
    }
    const launcher = path.join(project, "node_modules", ".bin", "ripr");
    assert.equal(fs.realpathSync(launcher), fs.realpathSync(path.join(launcherPackage, "bin", "ripr.cjs")));
    const wrapped = spawnSync(process.execPath, [launcher, ...args], options);
    const direct = spawnSync(installed, args, options);
    for (const run of [wrapped, direct]) {
      assert.ifError(run.error);
      assert.equal(run.status, 0, run.stderr.toString());
      assert.equal(run.signal, null);
    }
    const wrappedReplies = mcpReplies(wrapped.stdout);
    const results = wrappedReplies.map((reply) => reply.result);
    const list = analysisResult(results, version);
    assert.deepEqual(wrappedReplies, mcpReplies(direct.stdout));
    assert.throws(() => mcpReplies(Buffer.concat([Buffer.from("banner\n"), wrapped.stdout])));
    assert.throws(() => mcpReplies(Buffer.concat([wrapped.stdout, Buffer.from("\n")])));
    const emptyList = { ...results[3], structuredContent: { ...list, items: [] } };
    assert.throws(() => analysisResult([...results.slice(0, 3), emptyList], version), /changed Rust function/);
    fs.unlinkSync(installed);
    const marker = path.join(root, "ambient-called");
    fs.writeFileSync(path.join(foreign, "ripr"), `#!/bin/sh\ntouch '${marker}'\n`);
    fs.chmodSync(path.join(foreign, "ripr"), 0o755);
    const absent = spawnSync(process.execPath, [launcher, ...args], {
      ...options, encoding: "utf8", env: { ...process.env, PATH: `${foreign}${path.delimiter}${process.env.PATH || ""}` },
    });
    assert.ifError(absent.error);
    assert.equal(absent.status, 1);
    assert.equal(absent.signal, null);
    assert.equal(absent.stdout, "");
    assert.match(absent.stderr, /does not contain/);
    assert.equal(fs.existsSync(marker), false, "missing payload must not fall back to PATH");
    t.diagnostic(JSON.stringify({
      version,
      target: "x86_64-unknown-linux-gnu",
      launcherIntegrity: launcherPack.integrity,
      nativeIntegrity: nativePack.integrity,
      binarySha256,
      snapshotId: list.snapshot_id,
      gapIds: list.items.map((item) => item.canonical_id),
      requestBytes: Buffer.byteLength(request),
      stdoutBytes: { wrapped: wrapped.stdout.length, direct: direct.stdout.length },
      stderrBytes: { wrapped: wrapped.stderr.length, direct: direct.stderr.length },
    }));
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("the launcher bin remains executable in the checkout and packed package", (t) => {
  if (process.platform === "win32") {
    t.skip("Windows does not preserve POSIX executable mode bits");
    return;
  }

  const root = path.join(__dirname, "..");
  const bin = path.join(root, "bin", "ripr.cjs");
  assert.notEqual(fs.statSync(bin).mode & 0o111, 0, "bin/ripr.cjs must be executable in the checkout");

  const npmCli = process.env.npm_execpath;
  const command = npmCli ? process.execPath : "npm";
  const args = npmCli
    ? [npmCli, "pack", "--dry-run", "--json", "--ignore-scripts"]
    : ["pack", "--dry-run", "--json", "--ignore-scripts"];
  const result = spawnSync(command, args, { cwd: root, encoding: "utf8" });
  assert.equal(result.status, 0, result.stderr);

  const report = JSON.parse(result.stdout);
  const packedBin = report[0].files.find((entry) => entry.path === "bin/ripr.cjs");
  assert.ok(packedBin, "packed package must contain bin/ripr.cjs");
  assert.notEqual(packedBin.mode & 0o111, 0, "packed bin/ripr.cjs must be executable");
});
