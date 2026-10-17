"use strict";

const assert = require("node:assert/strict");
const { EventEmitter } = require("node:events");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawn, spawnSync } = require("node:child_process");
const test = require("node:test");
const { createRequire } = require("node:module");

const launcher = require("../lib/launcher.cjs");
const manifest = require("../package.json");

function fixtureRoot(name) {
  return fs.mkdtempSync(path.join(os.tmpdir(), `ripr-npm-${name}-`));
}

function writeJson(filePath, value) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`);
}

function launcherManifest(overrides = {}) {
  return { ...structuredClone(manifest), ...overrides };
}

function platformRow(packageName = "@effortlessmetrics/ripr-linux-x64-gnu") {
  return manifest.ripr.platforms.find((entry) => entry.package === packageName);
}

function nativeFixture(options = {}) {
  const root = fixtureRoot("native");
  const launcherRoot = path.join(root, "launcher");
  const selected = options.selected || platformRow();
  writeJson(path.join(launcherRoot, "package.json"), manifest);
  fs.mkdirSync(path.join(launcherRoot, "bin"), { recursive: true });
  fs.mkdirSync(path.join(launcherRoot, "lib"), { recursive: true });
  fs.copyFileSync(path.join(__dirname, "..", "bin", "ripr.cjs"), path.join(launcherRoot, "bin", "ripr.cjs"));
  fs.copyFileSync(path.join(__dirname, "..", "lib", "launcher.cjs"), path.join(launcherRoot, "lib", "launcher.cjs"));
  fs.chmodSync(path.join(launcherRoot, "bin", "ripr.cjs"), 0o755);
  const packageRoot = path.join(launcherRoot, "node_modules", ...selected.package.split("/"));
  const executableRelative = options.executable || selected.executable;
  writeJson(path.join(packageRoot, "package.json"), {
    name: options.name || selected.package,
    version: options.version || manifest.version,
    riprNative: {
      schemaVersion: 1,
      product: "ripr",
      target: options.target || selected.rustTarget,
      executable: executableRelative,
    },
  });
  if (options.createExecutable !== false) {
    const executablePath = path.join(packageRoot, ...executableRelative.split("/"));
    fs.mkdirSync(path.dirname(executablePath), { recursive: true });
    if (options.executableKind === "directory") {
      fs.mkdirSync(executablePath);
    } else if (options.executableKind === "symlink") {
      const target = path.join(root, "outside");
      fs.writeFileSync(target, "outside");
      fs.symlinkSync(target, executablePath);
    } else {
      fs.copyFileSync(process.execPath, executablePath);
      fs.chmodSync(executablePath, options.mode === undefined ? 0o755 : options.mode);
    }
  }
  return { root, launcherRoot, selected, packageRoot };
}

function cleanup(root) {
  fs.rmSync(root, { recursive: true, force: true });
}

function delay(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

async function waitForFile(filePath, timeoutMilliseconds = 5000) {
  const deadline = Date.now() + timeoutMilliseconds;
  while (Date.now() < deadline) {
    if (fs.existsSync(filePath)) {
      return fs.readFileSync(filePath, "utf8");
    }
    await delay(20);
  }
  throw new Error(`timed out waiting for ${filePath}`);
}

function waitForExit(child, timeoutMilliseconds = 10000) {
  return new Promise((resolve, reject) => {
    const stdout = [];
    const stderr = [];
    child.stdout?.on("data", (chunk) => stdout.push(chunk));
    child.stderr?.on("data", (chunk) => stderr.push(chunk));
    const timer = setTimeout(() => {
      reject(new Error(`timed out waiting for launcher process ${child.pid}`));
    }, timeoutMilliseconds);
    child.once("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });
    child.once("exit", (code, signal) => {
      clearTimeout(timer);
      resolve({
        code,
        signal,
        stdout: Buffer.concat(stdout).toString("utf8"),
        stderr: Buffer.concat(stderr).toString("utf8"),
      });
    });
  });
}

function signalProbe(root, signalName) {
  const script = path.join(root, `${signalName}-signal-probe.cjs`);
  fs.writeFileSync(
    script,
    `"use strict";\nconst fs = require("node:fs");\nconst signal = process.env.RIPR_SIGNAL_NAME;\nconst ready = process.env.RIPR_SIGNAL_READY;\nconst receipt = process.env.RIPR_SIGNAL_RECEIPT;\nlet count = 0;\nprocess.on(signal, () => {\n  count += 1;\n  fs.writeFileSync(receipt, JSON.stringify({ signal, count, pid: process.pid }));\n  if (count === 1) {\n    setTimeout(() => process.exit(0), 150);\n  }\n});\nfs.writeFileSync(ready, String(process.pid));\nsetInterval(() => {}, 1000);\n`,
  );
  return script;
}

function spawnSignalLauncher(fixture, signalName) {
  const ready = path.join(fixture.root, `${signalName}-ready.txt`);
  const receipt = path.join(fixture.root, `${signalName}-receipt.json`);
  const bin = path.join(fixture.launcherRoot, "bin", "ripr.cjs");
  const child = spawn(process.execPath, [bin, signalProbe(fixture.root, signalName)], {
    cwd: fixture.root,
    detached: true,
    env: {
      ...process.env,
      RIPR_SIGNAL_NAME: signalName,
      RIPR_SIGNAL_READY: ready,
      RIPR_SIGNAL_RECEIPT: receipt,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  return { child, ready, receipt };
}

function killProcessGroup(pid, signal = "SIGKILL") {
  try {
    process.kill(-pid, signal);
  } catch (error) {
    if (error.code !== "ESRCH") {
      throw error;
    }
  }
}

async function assertProcessGone(pid, timeoutMilliseconds = 3000) {
  const deadline = Date.now() + timeoutMilliseconds;
  while (Date.now() < deadline) {
    try {
      process.kill(pid, 0);
    } catch (error) {
      if (error.code === "ESRCH") {
        return;
      }
      throw error;
    }
    await delay(20);
  }
  throw new Error(`process ${pid} remained alive after launcher termination`);
}

function hostPlatformOrSkip(t) {
  try {
    return launcher.selectPlatform(launcher.validateLauncherManifest(manifest).platforms);
  } catch (error) {
    t.skip(`host platform is not a supported target: ${error.message}`);
    return null;
  }
}

test("validates the exact five-target contract", () => {
  const contract = launcher.validateLauncherManifest(manifest);
  assert.equal(contract.version, "0.11.0");
  assert.deepEqual(
    contract.platforms.map((entry) => launcher.platformKey(entry.nodePlatform, entry.nodeArch, entry.libc)),
    ["darwin/arm64/none", "darwin/x64/none", "linux/arm64/glibc", "linux/x64/glibc", "win32/x64/none"],
  );
  assert.deepEqual(
    Object.keys(manifest.optionalDependencies).sort(),
    contract.platforms.map((entry) => entry.package).sort(),
  );
});

test("rejects lifecycle scripts, version ranges, and dependency drift", () => {
  for (const scriptName of [
    "preinstall",
    "install",
    "postinstall",
    "preprepare",
    "prepare",
    "postprepare",
  ]) {
    const mutated = launcherManifest({
      scripts: { ...manifest.scripts, [scriptName]: "node should-not-run.cjs" },
    });
    assert.throws(() => launcher.validateLauncherManifest(mutated), {
      code: "install_script_forbidden",
    });
  }

  const ranged = launcherManifest({
    optionalDependencies: { ...manifest.optionalDependencies, [platformRow().package]: "^0.11.0" },
  });
  assert.throws(() => launcher.validateLauncherManifest(ranged), { code: "native_dependency_version_mismatch" });

  const extra = launcherManifest({
    optionalDependencies: { ...manifest.optionalDependencies, "@effortlessmetrics/ripr-extra": manifest.version },
  });
  assert.throws(() => launcher.validateLauncherManifest(extra), { code: "native_dependency_set_mismatch" });
});

test("detects glibc and rejects musl or unknown Linux libc", () => {
  assert.equal(launcher.detectLinuxLibc({ getReport: () => ({ header: { glibcVersionRuntime: "2.36" } }) }), "glibc");
  assert.equal(launcher.detectLinuxLibc({ getReport: () => ({ sharedObjects: ["/lib/ld-musl-x86_64.so.1"] }) }), "musl");
  assert.equal(launcher.detectLinuxLibc({ getReport: () => ({ header: {}, sharedObjects: [] }) }), "unknown");

  const contract = launcher.validateLauncherManifest(manifest);
  assert.equal(
    launcher.selectPlatform(contract.platforms, { platform: "linux", arch: "x64", libc: "glibc" }).rustTarget,
    "x86_64-unknown-linux-gnu",
  );
  assert.throws(() => launcher.selectPlatform(contract.platforms, { platform: "linux", arch: "x64", libc: "musl" }), {
    code: "platform_unsupported",
  });
  assert.throws(() => launcher.selectPlatform(contract.platforms, { platform: "freebsd", arch: "x64" }), {
    code: "platform_unsupported",
  });
});

test("resolves an exact native package and confines its executable", () => {
  const fixture = nativeFixture();
  try {
    const resolver = createRequire(path.join(fixture.launcherRoot, "package.json"));
    const resolved = launcher.resolveNativeExecutable({
      launcherRoot: fixture.launcherRoot,
      selectedPlatform: fixture.selected,
      resolver,
    });
    assert.equal(resolved.selected.package, fixture.selected.package);
    assert.equal(resolved.contract.version, manifest.version);
    assert.equal(fs.realpathSync(resolved.executablePath), resolved.executablePath);
    assert.ok(launcher.isPathInside(fs.realpathSync(fixture.packageRoot), resolved.executablePath));
  } finally {
    cleanup(fixture.root);
  }
});

test("distinguishes native resolution failures from missing optional packages", () => {
  const fixture = nativeFixture({ createExecutable: false });
  try {
    for (const code of ["MODULE_NOT_FOUND", "ERR_INVALID_PACKAGE_CONFIG", "ERR_PACKAGE_PATH_NOT_EXPORTED", "EACCES"]) {
      const cause = Object.assign(new Error(`resolution refused: ${code}`), { code });
      assert.throws(
        () => launcher.resolveNativeExecutable({
          launcherRoot: fixture.launcherRoot,
          selectedPlatform: fixture.selected,
          resolver: { resolve() { throw cause; } },
        }),
        (error) => {
          assert.ok(error instanceof launcher.LauncherError);
          assert.equal(error.code, code === "MODULE_NOT_FOUND" ? "native_package_missing" : "native_package_resolution_failed");
          assert.equal(error.cause, cause);
          assert.ok(error.message.includes(`${fixture.selected.package}@${manifest.version}`));
          assert.ok(error.message.includes(cause.message));
          assert.ok(error.message.includes(`reinstall ${manifest.name}@${manifest.version}`));
          return true;
        },
      );
    }
  } finally {
    cleanup(fixture.root);
  }
});

test("public bin retains missing-manifest and missing-exports reasons without claiming package absence", (t) => {
  const selected = hostPlatformOrSkip(t);
  if (!selected) {
    return;
  }
  const fixture = nativeFixture({ selected, createExecutable: false });
  const nativeManifestPath = path.join(fixture.packageRoot, "package.json");
  const originalManifest = JSON.parse(fs.readFileSync(nativeManifestPath, "utf8"));
  const bin = path.join(fixture.launcherRoot, "bin", "ripr.cjs");
  const fakePath = path.join(fixture.root, "decoy");
  fs.mkdirSync(fakePath);
  const fakeExecutable = path.join(fakePath, process.platform === "win32" ? "ripr.cmd" : "ripr");
  fs.writeFileSync(fakeExecutable, process.platform === "win32" ? "@echo PATH-FALLBACK\r\n" : "#!/bin/sh\necho PATH-FALLBACK\n");
  fs.chmodSync(fakeExecutable, 0o755);
  try {
    for (const variant of ["missing manifest", "missing exported target", "absent package"]) {
      if (variant === "missing manifest") {
        fs.unlinkSync(nativeManifestPath);
        assert.ok(fs.statSync(fixture.packageRoot).isDirectory());
      } else if (variant === "missing exported target") {
        writeJson(nativeManifestPath, { ...originalManifest, exports: { "./package.json": "./missing-manifest.json" } });
        assert.ok(fs.statSync(nativeManifestPath).isFile());
      } else {
        cleanup(fixture.packageRoot);
        assert.equal(fs.existsSync(fixture.packageRoot), false);
      }
      const result = spawnSync(process.execPath, [bin, "--version"], {
        cwd: fixture.root,
        env: { ...process.env, PATH: `${fakePath}${path.delimiter}${process.env.PATH || ""}` },
        encoding: "utf8",
      });
      assert.equal(result.status, 1, variant);
      assert.equal(result.stdout, "", variant);
      assert.ok(result.stderr.includes(`cannot resolve required native package ${selected.package}@${manifest.version}`), variant);
      assert.match(result.stderr, /Cannot find module/, variant);
      if (variant === "missing exported target") {
        assert.ok(result.stderr.includes("missing-manifest.json"));
      }
      assert.ok(result.stderr.includes(`reinstall ${manifest.name}@${manifest.version} with optional dependencies enabled`), variant);
      assert.doesNotMatch(result.stderr, /is missing;|PATH-FALLBACK/, variant);
    }
  } finally {
    cleanup(fixture.root);
  }
});

test("rejects non-object native manifests with a contextual typed error", () => {
  const fixture = nativeFixture({ createExecutable: false });
  const nativeManifestPath = path.join(fixture.packageRoot, "package.json");
  try {
    for (const value of [null, [], true, 5, "invalid"]) {
      writeJson(nativeManifestPath, value);
      assert.throws(
        () => launcher.resolveNativeExecutable({
          launcherRoot: fixture.launcherRoot,
          selectedPlatform: fixture.selected,
          resolver: { resolve() { return nativeManifestPath; } },
        }),
        (error) => {
          assert.ok(error instanceof launcher.LauncherError);
          assert.equal(error.code, "manifest_invalid");
          assert.equal(error.message, `native package ${fixture.selected.package} manifest must be an object`);
          return true;
        },
      );
    }
  } finally {
    cleanup(fixture.root);
  }
});

test("public bin identifies corrupt package metadata and recovers after restoration", (t) => {
  const selected = hostPlatformOrSkip(t);
  if (!selected) {
    return;
  }
  const fixture = nativeFixture({ selected, createExecutable: false });
  const nativeManifestPath = path.join(fixture.packageRoot, "package.json");
  const originalManifest = fs.readFileSync(nativeManifestPath);
  const bin = path.join(fixture.launcherRoot, "bin", "ripr.cjs");
  try {
    fs.writeFileSync(nativeManifestPath, "{broken\n");
    const corrupt = spawnSync(process.execPath, [bin, "--version"], { cwd: fixture.root, encoding: "utf8" });
    assert.equal(corrupt.status, 1);
    assert.equal(corrupt.stdout, "");
    assert.ok(corrupt.stderr.includes(`cannot resolve required native package ${selected.package}@${manifest.version}`));
    assert.match(corrupt.stderr, /Invalid package|Error parsing/);
    assert.ok(corrupt.stderr.includes(`reinstall ${manifest.name}@${manifest.version}`));
    assert.doesNotMatch(corrupt.stderr, /is missing;|Cannot read properties/);

    fs.writeFileSync(nativeManifestPath, originalManifest);
    const restored = spawnSync(process.execPath, [bin, "--version"], { cwd: fixture.root, encoding: "utf8" });
    assert.equal(restored.status, 1);
    assert.equal(restored.stdout, "");
    assert.ok(restored.stderr.includes(`does not contain ${selected.executable}`));
    assert.doesNotMatch(restored.stderr, /cannot resolve|is missing;|Cannot read properties/);
  } finally {
    cleanup(fixture.root);
  }
});

test("rejects missing, wrong-version, wrong-target, traversal, symlink, directory, and non-executable payloads", () => {
  const missing = nativeFixture({ createExecutable: false });
  try {
    assert.throws(
      () => launcher.resolveNativeExecutable({ launcherRoot: missing.launcherRoot, selectedPlatform: missing.selected }),
      { code: "native_executable_missing" },
    );
  } finally {
    cleanup(missing.root);
  }

  for (const [name, options, code] of [
    ["version", { version: "0.11.1" }, "native_package_version_mismatch"],
    ["target", { target: "aarch64-unknown-linux-gnu" }, "native_target_mismatch"],
    ["traversal", { executable: "../ripr" }, "native_executable_mismatch"],
    ["symlink", { executableKind: "symlink" }, "native_executable_symlink"],
    ["directory", { executableKind: "directory" }, "native_executable_not_file"],
  ]) {
    const fixture = nativeFixture(options);
    try {
      assert.throws(
        () => launcher.resolveNativeExecutable({ launcherRoot: fixture.launcherRoot, selectedPlatform: fixture.selected }),
        { code },
        name,
      );
    } finally {
      cleanup(fixture.root);
    }
  }

  if (process.platform !== "win32") {
    const nonExecutable = nativeFixture({ mode: 0o644 });
    try {
      assert.throws(
        () => launcher.resolveNativeExecutable({ launcherRoot: nonExecutable.launcherRoot, selectedPlatform: nonExecutable.selected }),
        { code: "native_executable_not_executable" },
      );
    } finally {
      cleanup(nonExecutable.root);
    }
  }
});

test("missing native executable gives exact recovery for every registered target", () => {
  for (const selected of manifest.ripr.platforms) {
    const fixture = nativeFixture({ selected, createExecutable: false });
    try {
      assert.ok(fs.statSync(path.join(fixture.packageRoot, "package.json")).isFile());
      assert.equal(fs.existsSync(path.join(fixture.packageRoot, ...selected.executable.split("/"))), false);
      assert.throws(
        () => launcher.resolveNativeExecutable({ launcherRoot: fixture.launcherRoot, selectedPlatform: selected }),
        (error) => {
          assert.ok(error instanceof launcher.LauncherError);
          assert.equal(error.code, "native_executable_missing");
          assert.equal(error.cause.code, "ENOENT");
          assert.equal(error.message,
            `native package ${selected.package}@${manifest.version} does not contain ${selected.executable}; restore ${selected.executable} from the matching native package tarball, or reinstall ${manifest.name}@${manifest.version} in its owning npm environment with optional dependencies enabled`);
          return true;
        },
      );
    } finally {
      cleanup(fixture.root);
    }
  }
});

test("public bin missing-native failure gives exact recovery after a directory move", (t) => {
  const selected = hostPlatformOrSkip(t);
  if (!selected) return;
  const fixture = nativeFixture({ selected, createExecutable: false });
  const foreignCwd = fixtureRoot("foreign native 工作 café");
  const version = "0.11.0-rc.7";
  const exactManifest = launcherManifest({ version });
  for (const name of Object.keys(exactManifest.optionalDependencies)) exactManifest.optionalDependencies[name] = version;
  writeJson(path.join(fixture.launcherRoot, "package.json"), exactManifest);
  const nativeManifestPath = path.join(fixture.packageRoot, "package.json");
  writeJson(nativeManifestPath, { ...JSON.parse(fs.readFileSync(nativeManifestPath, "utf8")), version });
  writeJson(path.join(foreignCwd, "package.json"), { name: manifest.name, version: "9.9.9" });
  const decoy = path.join(foreignCwd, process.platform === "win32" ? "ripr.cmd" : "ripr");
  fs.writeFileSync(decoy, process.platform === "win32" ? "@echo PATH-FALLBACK\r\n" : "#!/bin/sh\necho PATH-FALLBACK\n");
  fs.chmodSync(decoy, 0o755);
  try {
    for (const moved of [false, true]) {
      if (moved) {
        const newRoot = `${fixture.root} moved 工作 café`;
        fs.renameSync(fixture.root, newRoot);
        fixture.root = newRoot;
        fixture.launcherRoot = path.join(newRoot, "launcher");
        fixture.packageRoot = path.join(fixture.launcherRoot, "node_modules", ...selected.package.split("/"));
      }
      const executable = path.join(fixture.packageRoot, ...selected.executable.split("/"));
      const bin = path.join(fixture.launcherRoot, "bin", "ripr.cjs");
      assert.equal(fs.existsSync(executable), false);
      const options = {
        cwd: foreignCwd,
        env: { ...process.env, PATH: `${foreignCwd}${path.delimiter}${process.env.PATH || ""}` },
        encoding: "utf8",
        timeout: 5000,
      };
      const missing = spawnSync(process.execPath, [bin, "--version"], options);
      assert.ifError(missing.error);
      assert.equal(missing.status, 1);
      assert.equal(missing.stdout, "");
      assert.equal(missing.stderr,
        `ripr npm launcher: native package ${selected.package}@${version} does not contain ${selected.executable}; restore ${selected.executable} from the matching native package tarball, or reinstall ${manifest.name}@${version} in its owning npm environment with optional dependencies enabled\n`);
      assert.doesNotMatch(missing.stderr, /PATH-FALLBACK|9\.9\.9|latest|node:internal/);

      // Node bytes exercise recovery/transport only, not a qualified RIPR payload.
      fs.mkdirSync(path.dirname(executable), { recursive: true });
      fs.copyFileSync(process.execPath, executable);
      fs.chmodSync(executable, 0o755);
      const restored = spawnSync(process.execPath, [bin, "-e", "process.exit(23)"], options);
      assert.ifError(restored.error);
      assert.equal(restored.status, 23);
      assert.equal(restored.stdout, "");
      assert.equal(restored.stderr, "");
      fs.unlinkSync(executable);
    }
  } finally {
    cleanup(fixture.root);
    cleanup(foreignCwd);
  }
});

test("public bin simulated unsupported platforms fail without reinstall advice or stdout", () => {
  const fixture = nativeFixture({ createExecutable: false });
  const preload = path.join(fixture.root, "simulated-platform.cjs");
  try {
    for (const [platform, arch, report, reason] of [
      ["linux", "x64", { sharedObjects: ["/lib/ld-musl-x86_64.so.1"] }, "unsupported Linux libc musl; this release provides glibc payloads only"],
      ["linux", "x64", { header: {}, sharedObjects: [] }, "unsupported Linux libc unknown; this release provides glibc payloads only"],
      ["freebsd", "x64", {}, "unsupported platform freebsd/x64/none"],
      ["win32", "arm64", {}, "unsupported platform win32/arm64/none"],
      ["linux", "riscv64", { header: { glibcVersionRuntime: "2.36" } }, "unsupported platform linux/riscv64/glibc"],
    ]) {
      fs.writeFileSync(preload,
        `Object.defineProperty(process, "platform", { value: ${JSON.stringify(platform)} });\nObject.defineProperty(process, "arch", { value: ${JSON.stringify(arch)} });\nprocess.report.getReport = () => (${JSON.stringify(report)});\n`);
      const result = spawnSync(process.execPath, ["--require", preload, path.join(fixture.launcherRoot, "bin", "ripr.cjs"), "--version"], {
        cwd: fixture.root,
        encoding: "utf8",
        timeout: 5000,
      });
      assert.ifError(result.error);
      assert.equal(result.status, 1);
      assert.equal(result.stdout, "");
      assert.equal(result.stderr, `ripr npm launcher: ${reason}\n`);
      assert.doesNotMatch(result.stderr, /reinstall|restore|tarball|node:internal/);
    }
  } finally {
    cleanup(fixture.root);
  }
});

test("spawns an absolute executable with literal argv, inherited stdio, cwd, env, and no shell", async () => {
  const child = new EventEmitter();
  child.killed = false;
  child.kill = () => true;
  let call;
  const spawnImpl = (executable, argv, options) => {
    call = { executable, argv, options };
    queueMicrotask(() => child.emit("exit", 23, null));
    return child;
  };
  const env = { RIPR_TEST_VALUE: "present" };
  const cwd = path.resolve(os.tmpdir());
  const argv = ["--flag", "$(touch should-not-run)", "semi;colon", "space value"];
  const result = await launcher.runNative(path.resolve(process.execPath), argv, {
    spawnImpl,
    cwd,
    env,
    forwardSignals: false,
  });
  assert.deepEqual(result, { code: 23, signal: null });
  assert.equal(call.executable, path.resolve(process.execPath));
  assert.deepEqual(call.argv, argv);
  assert.equal(call.options.shell, false);
  assert.equal(call.options.stdio, "inherit");
  assert.equal(call.options.cwd, cwd);
  assert.equal(call.options.env, env);
});

test("maps child signals to a nonzero conventional exit code", () => {
  assert.equal(launcher.signalExitCode("SIGTERM"), 128 + os.constants.signals.SIGTERM);
  assert.equal(launcher.signalExitCode("RIPR_UNKNOWN_SIGNAL"), 1);
});

test("launch sets a nonzero exit code before forwarding a child signal", async () => {
  const fixture = nativeFixture();
  const child = new EventEmitter();
  child.killed = false;
  child.kill = () => true;
  const spawnImpl = () => {
    queueMicrotask(() => child.emit("exit", null, "SIGTERM"));
    return child;
  };
  const resolver = createRequire(path.join(fixture.launcherRoot, "package.json"));
  const originalKill = process.kill;
  const originalExitCode = process.exitCode;
  const forwarded = [];
  process.kill = (pid, signal) => {
    forwarded.push({ pid, signal, exitCode: process.exitCode });
    return true;
  };
  process.exitCode = undefined;
  try {
    await launcher.launch({
      launcherRoot: fixture.launcherRoot,
      selectedPlatform: fixture.selected,
      resolver,
      spawnImpl,
      forwardSignals: false,
    });
    assert.equal(process.exitCode, 128 + os.constants.signals.SIGTERM);
    assert.deepEqual(forwarded, [
      { pid: process.pid, signal: "SIGTERM", exitCode: 128 + os.constants.signals.SIGTERM },
    ]);
  } finally {
    process.kill = originalKill;
    process.exitCode = originalExitCode;
    cleanup(fixture.root);
  }
});

test(
  "forwards direct SIGTERM to native child exactly once and re-emits signal",
  { skip: process.platform === "win32", timeout: 15000 },
  async () => {
    const fixture = nativeFixture();
    const { child, ready, receipt } = spawnSignalLauncher(fixture, "SIGTERM");
    let nativePid;
    try {
      nativePid = Number.parseInt(await waitForFile(ready), 10);
      assert.ok(Number.isInteger(nativePid));
      const exitPromise = waitForExit(child);
      process.kill(child.pid, "SIGTERM");
      const exit = await exitPromise;
      await waitForFile(receipt);
      await delay(250);
      assert.deepEqual(JSON.parse(fs.readFileSync(receipt, "utf8")), {
        signal: "SIGTERM",
        count: 1,
        pid: nativePid,
      });
      assert.equal(exit.signal, "SIGTERM");
      assert.equal(exit.stdout, "");
      assert.equal(exit.stderr, "");
      await assertProcessGone(nativePid);
    } finally {
      killProcessGroup(child.pid);
      cleanup(fixture.root);
    }
  },
);

test(
  "retains the first observed signal when a supervisor escalates SIGINT to SIGTERM",
  { skip: process.platform === "win32", timeout: 15000 },
  async () => {
    const fixture = nativeFixture();
    const { child, ready, receipt } = spawnSignalLauncher(fixture, "SIGTERM");
    let nativePid;
    try {
      nativePid = Number.parseInt(await waitForFile(ready), 10);
      assert.ok(Number.isInteger(nativePid));
      const exitPromise = waitForExit(child);
      process.kill(child.pid, "SIGINT");
      await delay(300);
      assert.ok(!fs.existsSync(receipt), "observed SIGINT must not be forwarded to the native child");
      process.kill(child.pid, "SIGTERM");
      const exit = await exitPromise;
      await waitForFile(receipt);
      await delay(250);
      assert.deepEqual(JSON.parse(fs.readFileSync(receipt, "utf8")), {
        signal: "SIGTERM",
        count: 1,
        pid: nativePid,
      });
      assert.equal(exit.signal, "SIGINT");
      assert.equal(exit.stdout, "");
      assert.equal(exit.stderr, "");
      await assertProcessGone(nativePid);
    } finally {
      killProcessGroup(child.pid);
      cleanup(fixture.root);
    }
  },
);

test(
  "observes terminal SIGINT and SIGHUP without forwarding duplicates to the native child",
  { skip: process.platform === "win32", timeout: 30000 },
  async () => {
    for (const signalName of ["SIGINT", "SIGHUP"]) {
      const fixture = nativeFixture();
      const { child, ready, receipt } = spawnSignalLauncher(fixture, signalName);
      let nativePid;
      try {
        nativePid = Number.parseInt(await waitForFile(ready), 10);
        assert.ok(Number.isInteger(nativePid));
        const exitPromise = waitForExit(child);
        process.kill(-child.pid, signalName);
        const exit = await exitPromise;
        await waitForFile(receipt);
        await delay(250);
        assert.deepEqual(JSON.parse(fs.readFileSync(receipt, "utf8")), {
          signal: signalName,
          count: 1,
          pid: nativePid,
        });
        assert.equal(exit.signal, signalName);
        assert.equal(exit.stdout, "");
        assert.equal(exit.stderr, "");
        await assertProcessGone(nativePid);
      } finally {
        killProcessGroup(child.pid);
        cleanup(fixture.root);
      }
    }
  },
);

test("runs a real synthetic executable and preserves argv, cwd, env, stdout, stderr, and exit status", (t) => {
  const selected = hostPlatformOrSkip(t);
  if (!selected) {
    return;
  }
  const fixture = nativeFixture({ selected });
  const script = path.join(fixture.root, "probe.cjs");
  fs.writeFileSync(
    script,
    `const fs = require("node:fs");\nfs.writeFileSync(process.env.RIPR_PROBE_OUT, JSON.stringify({argv: process.argv.slice(2), cwd: process.cwd(), env: process.env.RIPR_PROBE_VALUE}));\nprocess.stdout.write("probe-stdout\\n");\nprocess.stderr.write("probe-stderr\\n");\nprocess.exit(23);\n`,
  );
  const out = path.join(fixture.root, "probe.json");
  const bin = path.join(fixture.launcherRoot, "bin", "ripr.cjs");
  const result = spawnSync(process.execPath, [bin, script, "literal;value", "space value"], {
    cwd: fixture.root,
    env: {
      ...process.env,
      RIPR_PROBE_OUT: out,
      RIPR_PROBE_VALUE: "present",
    },
    encoding: "utf8",
  });
  try {
    assert.equal(result.status, 23, result.stderr);
    assert.equal(result.stdout, "probe-stdout\n");
    assert.equal(result.stderr, "probe-stderr\n");
    assert.deepEqual(JSON.parse(fs.readFileSync(out, "utf8")), {
      argv: ["literal;value", "space value"],
      cwd: fixture.root,
      env: "present",
    });
  } finally {
    cleanup(fixture.root);
  }
});

test("source bin missing-package failure never falls back to PATH or writes stdout", (t) => {
  if (!hostPlatformOrSkip(t)) {
    return;
  }
  const fakePath = fixtureRoot("path-fallback");
  const foreignCwd = fixtureRoot("foreign-cwd");
  const fakeExecutable = path.join(fakePath, process.platform === "win32" ? "ripr.cmd" : "ripr");
  fs.writeFileSync(fakeExecutable, process.platform === "win32" ? "@echo PATH-FALLBACK\r\n" : "#!/bin/sh\necho PATH-FALLBACK\n");
  fs.chmodSync(fakeExecutable, 0o755);
  const bin = path.join(__dirname, "..", "bin", "ripr.cjs");
  const result = spawnSync(process.execPath, [bin, "--version"], {
    cwd: foreignCwd,
    env: { ...process.env, PATH: `${fakePath}${path.delimiter}${process.env.PATH || ""}` },
    encoding: "utf8",
  });
  try {
    assert.equal(result.status, 1);
    assert.equal(result.stdout, "");
    assert.doesNotMatch(result.stdout + result.stderr, /PATH-FALLBACK/);
    assert.match(result.stderr, /cannot resolve required native package .*@0\.11\.0: Cannot find module/);
    assert.match(result.stderr, /reinstall .*@0\.11\.0 with optional dependencies enabled/);
  } finally {
    cleanup(fakePath);
    cleanup(foreignCwd);
  }
});

test("public bin missing-library failure gives exact recovery after a directory move", (t) => {
  const selected = hostPlatformOrSkip(t);
  if (!selected) return;
  const fixture = nativeFixture({ selected, createExecutable: false });
  const foreignCwd = fixtureRoot("foreign library 工作 café");
  const libraryBytes = fs.readFileSync(path.join(fixture.launcherRoot, "lib", "launcher.cjs"));
  writeJson(path.join(foreignCwd, "package.json"), { name: "foreign-package", version: "9.9.9" });
  const decoy = path.join(foreignCwd, process.platform === "win32" ? "ripr.cmd" : "ripr");
  fs.writeFileSync(decoy, process.platform === "win32" ? "@echo PATH-FALLBACK\r\n" : "#!/bin/sh\necho PATH-FALLBACK\n");
  fs.chmodSync(decoy, 0o755);
  try {
    for (const moved of [false, true]) {
      if (moved) {
        const newRoot = `${fixture.root} moved 工作 café`;
        fs.renameSync(fixture.root, newRoot);
        fixture.root = newRoot;
        fixture.launcherRoot = path.join(newRoot, "launcher");
      }
      const library = path.join(fixture.launcherRoot, "lib", "launcher.cjs");
      const bin = path.join(fixture.launcherRoot, "bin", "ripr.cjs");
      fs.unlinkSync(library);
      const options = {
        cwd: foreignCwd,
        env: { ...process.env, PATH: `${foreignCwd}${path.delimiter}${process.env.PATH || ""}` },
        encoding: "utf8",
      };
      const result = spawnSync(process.execPath, [bin, "--version"], options);
      assert.equal(result.status, 1);
      assert.equal(result.stdout, "");
      assert.match(result.stderr, /^ripr npm launcher: cannot resolve packaged launcher library lib\/launcher\.cjs:/);
      assert.match(result.stderr, /Cannot find module '\.\.\/lib\/launcher\.cjs'/);
      assert.match(result.stderr, /restore lib\/launcher\.cjs from the matching package tarball/);
      assert.match(result.stderr, /reinstall @effortlessmetrics\/ripr@0\.11\.0 in its owning npm environment/);
      assert.doesNotMatch(result.stderr, /node:internal|PATH-FALLBACK|foreign-package|9\.9\.9/);
      fs.writeFileSync(library, libraryBytes);
      assert.deepEqual(fs.readFileSync(library), libraryBytes);
      const restored = spawnSync(process.execPath, [bin, "--version"], options);
      assert.equal(restored.status, 1);
      assert.equal(restored.stdout, "");
      assert.match(restored.stderr, /^ripr npm launcher: native package .*@0\.11\.0 does not contain bin\/ripr/);
      assert.doesNotMatch(restored.stderr, /packaged launcher library|PATH-FALLBACK/);
    }
  } finally {
    cleanup(fixture.root);
    cleanup(foreignCwd);
  }
});

test("public bin preserves internal import and parser failures without library recovery advice", () => {
  const fixture = nativeFixture({ createExecutable: false });
  try {
    for (const [source, reason] of [
      ['require("ripr-deliberately-absent-internal-module");\n', /Cannot find module 'ripr-deliberately-absent-internal-module'/],
      ["module.exports = ;\n", /Unexpected token ';'/],
    ]) {
      fs.writeFileSync(path.join(fixture.launcherRoot, "lib", "launcher.cjs"), source);
      const result = spawnSync(process.execPath, [path.join(fixture.launcherRoot, "bin", "ripr.cjs"), "--version"], {
        cwd: fixture.root,
        encoding: "utf8",
      });
      assert.equal(result.status, 1);
      assert.equal(result.stdout, "");
      assert.match(result.stderr, /^ripr npm launcher: /);
      assert.match(result.stderr, reason);
      assert.doesNotMatch(result.stderr, /cannot resolve packaged launcher library|restore lib\/launcher\.cjs|reinstall|node:internal/);
    }
  } finally {
    cleanup(fixture.root);
  }
});

test("public bin library recovery never guesses a version from invalid or unavailable metadata", () => {
  const fixture = nativeFixture({ createExecutable: false });
  try {
    fs.unlinkSync(path.join(fixture.launcherRoot, "lib", "launcher.cjs"));
    for (const metadata of [
      { name: "foreign-package", version: "0.11.0" },
      { name: manifest.name, version: 11 },
      { name: manifest.name, version: "" },
      { name: manifest.name, version: "^0.11.0" },
      { name: manifest.name, version: "latest" },
      { name: manifest.name, version: "0.11.0-01" },
      { name: manifest.name, version: "0.11.0-rc.01" },
      { name: manifest.name, version: "9007199254740992.0.0" },
      { name: manifest.name, version: "0.9007199254740992.0" },
      { name: manifest.name, version: "0.0.9007199254740992" },
      { name: manifest.name, version: "0.11.0\n" },
      { name: manifest.name, version: "v0.11.0" },
      { name: manifest.name, version: `0.11.0-${"a".repeat(250)}` },
      null,
      undefined,
    ]) {
      const file = path.join(fixture.launcherRoot, "package.json");
      if (metadata === undefined) fs.unlinkSync(file);
      else writeJson(file, metadata);
      const result = spawnSync(process.execPath, [path.join(fixture.launcherRoot, "bin", "ripr.cjs"), "--version"], {
        cwd: fixture.root,
        encoding: "utf8",
      });
      assert.equal(result.status, 1);
      assert.equal(result.stdout, "");
      if (metadata === null) {
        // Node 20 and 24 refuse this shape differently before display lookup.
        let resolutionFailure;
        assert.throws(
          () => createRequire(path.join(fixture.launcherRoot, "bin", "ripr.cjs")).resolve("../lib/launcher.cjs"),
          (error) => {
            resolutionFailure = error;
            return error.code !== "MODULE_NOT_FOUND";
          },
        );
        assert.equal(result.stderr, `ripr npm launcher: ${resolutionFailure.message}\n`);
        assert.doesNotMatch(result.stderr, /packaged launcher library|restore lib\/launcher\.cjs|reinstall|node:internal/);
        continue;
      }
      assert.match(result.stderr, /^ripr npm launcher: cannot resolve packaged launcher library/);
      assert.match(result.stderr, /reinstall the exact @effortlessmetrics\/ripr version selected for this npm installation in its owning npm environment/);
      assert.doesNotMatch(result.stderr, /foreign-package|@ripr@|\^0\.11\.0|latest|node:internal/);
    }
    for (const version of [
      "0.11.0-0", "0.11.0-rc.1", "0.11.0-01alpha", "0.11.0+01", "0.11.0-rc.1+build.02",
      "9007199254740991.0.0", `0.11.0-${"a".repeat(249)}`,
    ]) {
      writeJson(path.join(fixture.launcherRoot, "package.json"), { name: manifest.name, version });
      const result = spawnSync(process.execPath, [path.join(fixture.launcherRoot, "bin", "ripr.cjs"), "--version"], {
        cwd: fixture.root,
        encoding: "utf8",
      });
      assert.equal(result.status, 1);
      assert.equal(result.stdout, "");
      assert.ok(result.stderr.includes(`reinstall ${manifest.name}@${version} in its owning npm environment`), result.stderr);
      assert.match(result.stderr, /^ripr npm launcher: cannot resolve packaged launcher library/);
      assert.doesNotMatch(result.stderr, /node:internal/);
    }
  } finally {
    cleanup(fixture.root);
  }
});

test("npm package contents are explicit and exclude tests and build residue", () => {
  const root = path.join(__dirname, "..");
  const npmCli = process.env.npm_execpath;
  const command = npmCli ? process.execPath : "npm";
  const args = npmCli
    ? [npmCli, "pack", "--dry-run", "--json", "--ignore-scripts"]
    : ["pack", "--dry-run", "--json", "--ignore-scripts"];
  const result = spawnSync(command, args, {
    cwd: root,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr);
  const report = JSON.parse(result.stdout);
  const files = report[0].files.map((entry) => entry.path).sort();
  assert.deepEqual(files, [
    "LICENSE-APACHE",
    "LICENSE-MIT",
    "README.md",
    "bin/ripr.cjs",
    "lib/launcher.cjs",
    "package.json",
  ]);
  assert.equal(report[0].name, "@effortlessmetrics/ripr");
  assert.equal(report[0].version, manifest.version);
});
