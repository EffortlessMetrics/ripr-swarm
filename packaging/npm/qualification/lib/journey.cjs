"use strict";

// Clean npm install-to-use TypeScript journey (#4627) through an isolated
// loopback registry (#4632). Every step runs real npm against staged
// launcher/native tarballs with lifecycle scripts disabled, a scrubbed PATH
// and a private npm configuration; the result is a typed
// `package_qualification_receipt` for `cargo xtask package-qualification-gate`.

const crypto = require("node:crypto");
const fs = require("node:fs");
const net = require("node:net");
const os = require("node:os");
const path = require("node:path");
const { spawn, spawnSync } = require("node:child_process");

const { stageTarball } = require("./registry.cjs");

const LAUNCHER = "@effortlessmetrics/ripr";
const LOG_TAIL_BYTES = 4096;
const STEP_TIMEOUT_MS = 180000;
const CLAIM_BOUNDARY = "This receipt records no-publish package-qualification evidence for one exact candidate. It does not publish, sign, promote support, authorize a registry, or qualify a different source tree.";
const NON_CLAIMS = [
  "not a public registry publication",
  "not a support-tier promotion",
  "not a runtime mutation result",
  "not authorization to publish",
];

// The TypeScript subject: a discount boundary moved from exclusive to
// inclusive while the tests only probe 9999 and 10001 (the
// blind_journey_installed_typescript base/head pair). RIPR must report it.
const SUBJECT_BASE = `/** Discounted total applies only above the discount threshold. */
export const DISCOUNT_THRESHOLD = 10000;

export function discountedTotal(subtotal: number): number {
  if (subtotal > DISCOUNT_THRESHOLD) {
    return subtotal - 500;
  }
  return subtotal;
}
`;
const SUBJECT_HEAD = SUBJECT_BASE.replace("subtotal > DISCOUNT_THRESHOLD", "subtotal >= DISCOUNT_THRESHOLD");
const SUBJECT_TEST = `import { describe, expect, it } from "vitest";

import { discountedTotal } from "../src/pricing";

describe("discountedTotal", () => {
  it("leaves a below-threshold subtotal unchanged", () => {
    expect(discountedTotal(9999)).toBe(9999);
  });

  it("discounts an above-threshold subtotal", () => {
    expect(discountedTotal(10001)).toBe(9501);
  });
});
`;

class JourneyFailure extends Error {
  constructor(message) {
    super(message);
    this.name = "JourneyFailure";
  }
}

function fail(message) {
  throw new JourneyFailure(message);
}

function tail(text) {
  const value = String(text || "");
  return value.length > LOG_TAIL_BYTES ? `...${value.slice(-LOG_TAIL_BYTES)}` : value;
}

function sha256File(file) {
  return `sha256:${crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex")}`;
}

function sha256Bytes(bytes) {
  return `sha256:${crypto.createHash("sha256").update(bytes).digest("hex")}`;
}

function isWindows(platform) {
  return platform === "win32";
}

// PATH lookup without a shell; honours PATHEXT on Windows.
function which(name, searchPath, platform = process.platform) {
  const separator = isWindows(platform) ? ";" : ":";
  const extensions = isWindows(platform)
    ? (process.env.PATHEXT || ".COM;.EXE;.BAT;.CMD").split(";").filter(Boolean)
    : [""];
  for (const dir of String(searchPath || "").split(separator).filter(Boolean)) {
    for (const extension of extensions) {
      const candidate = path.join(dir, name + extension);
      try {
        if (fs.statSync(candidate).isFile()) {
          if (!isWindows(platform)) fs.accessSync(candidate, fs.constants.X_OK);
          return candidate;
        }
      } catch {
        // keep looking
      }
    }
  }
  return null;
}

function npmCli() {
  const nodeDir = path.dirname(process.execPath);
  const candidates = [
    process.env.npm_execpath,
    path.join(nodeDir, "node_modules", "npm", "bin", "npm-cli.js"),
    path.join(nodeDir, "..", "lib", "node_modules", "npm", "bin", "npm-cli.js"),
  ];
  // Distribution layouts (for example /usr/share/nodejs/npm) expose npm only
  // as a PATH entry that links to the CLI script; follow that link on POSIX.
  if (process.platform !== "win32") {
    const onPath = which("npm", process.env.PATH);
    if (onPath) {
      try {
        candidates.push(fs.realpathSync(onPath));
      } catch {
        // an unresolvable link is not a candidate
      }
    }
  }
  const found = candidates
    .filter((item) => typeof item === "string" && item.endsWith("npm-cli.js"))
    .find((item) => fs.existsSync(item));
  if (!found) fail(`cannot locate npm-cli.js next to ${process.execPath} or through npm on PATH`);
  return found;
}

// Splits a continuation command printed by ripr into argv. Accepts the two
// quoting forms ripr emits (agent/loop_commands.rs shell_arg_for): POSIX
// single quotes with the '\'' escape, and Windows console double quotes with
// "" doubling and a doubled trailing backslash run. Anything else (such as the
// $(printf ...) escape form) is refused rather than guessed.
function splitCommand(command) {
  const tokens = [];
  let current = null;
  let index = 0;
  const text = String(command);
  while (index < text.length) {
    const char = text[index];
    if (char === " " || char === "\t") {
      if (current !== null) tokens.push(current);
      current = null;
      index += 1;
    } else if (char === "'") {
      const end = text.indexOf("'", index + 1);
      if (end === -1) fail(`unterminated single quote in ${JSON.stringify(text)}`);
      current = (current || "") + text.slice(index + 1, end);
      index = end + 1;
    } else if (char === "\\" && text[index + 1] === "'") {
      current = (current || "") + "'";
      index += 2;
    } else if (char === "\"") {
      let body = "";
      index += 1;
      for (;;) {
        if (index >= text.length) fail(`unterminated double quote in ${JSON.stringify(text)}`);
        if (text[index] === "$" || text[index] === "`") {
          fail(`continuation ${JSON.stringify(text)} uses shell expansion this journey does not replay`);
        }
        if (text[index] === "\"" && text[index + 1] === "\"") {
          body += "\"";
          index += 2;
        } else if (text[index] === "\"") {
          index += 1;
          break;
        } else {
          body += text[index];
          index += 1;
        }
      }
      const run = /\\+$/.exec(body);
      if (run && run[0].length % 2 === 0) body = body.slice(0, body.length - run[0].length / 2);
      current = (current || "") + body;
    } else if (char === "$" || char === "`") {
      fail(`continuation ${JSON.stringify(text)} uses shell expansion this journey does not replay`);
    } else {
      current = (current || "") + char;
      index += 1;
    }
  }
  if (current !== null) tokens.push(current);
  return tokens;
}

function parseArgs(argv) {
  const options = { keep: false };
  const take = (index, flag) => {
    const value = argv[index + 1];
    if (value === undefined) fail(`${flag} requires a value`);
    return value;
  };
  const valued = {
    "--packages": "packages",
    "--target": "target",
    "--version": "version",
    "--receipt": "receipt",
    "--log": "log",
    "--source-commit": "sourceCommit",
    "--source-tree": "sourceTree",
    "--expected-version-line": "expectedVersionLine",
    "--work": "work",
  };
  let index = 0;
  while (index < argv.length) {
    const flag = argv[index];
    if (flag === "--keep") {
      options.keep = true;
      index += 1;
    } else if (Object.hasOwn(valued, flag)) {
      options[valued[flag]] = take(index, flag);
      index += 2;
    } else {
      fail(`unknown argument ${flag}`);
    }
  }
  for (const required of ["packages", "target", "version", "receipt", "sourceCommit", "sourceTree"]) {
    if (!options[required]) fail(`--${required.replace(/[A-Z]/g, (letter) => `-${letter.toLowerCase()}`)} is required`);
  }
  for (const [field, value] of [["--source-commit", options.sourceCommit], ["--source-tree", options.sourceTree]]) {
    if (value !== undefined && !/^[0-9a-f]{40}$/.test(value)) fail(`${field} must be a 40-character lowercase git SHA`);
  }
  return options;
}

// Picks the launcher and the target's native tarball from the staged set and
// reads the payload identity from the native tarball's own bytes.
function stagedIdentities(registry, options) {
  const launcher = registry.staged.find((item) => item.manifest.name === LAUNCHER && item.manifest.version === options.version);
  if (!launcher) fail(`no staged ${LAUNCHER}@${options.version} tarball`);
  const contract = launcher.manifest.ripr;
  const platforms = contract && Array.isArray(contract.platforms) ? contract.platforms : [];
  const platform = platforms.find((entry) => entry.rustTarget === options.target);
  if (!platform) fail(`${LAUNCHER}@${options.version} declares no platform for ${options.target}`);
  const optional = launcher.manifest.optionalDependencies || {};
  for (const entry of platforms) {
    if (optional[entry.package] !== options.version) {
      fail(`${LAUNCHER} must pin ${entry.package} to exactly ${options.version}, got ${JSON.stringify(optional[entry.package])}`);
    }
  }
  if (launcher.manifest.dependencies && Object.keys(launcher.manifest.dependencies).length > 0) {
    fail(`${LAUNCHER} must not declare runtime dependencies`);
  }
  const native = registry.staged.find((item) => item.manifest.name === platform.package && item.manifest.version === options.version);
  if (!native) fail(`no staged ${platform.package}@${options.version} tarball for ${options.target}`);
  const metadata = native.manifest.riprNative || {};
  if (metadata.target !== options.target || metadata.executable !== platform.executable) {
    fail(`${platform.package} riprNative metadata must name ${options.target} and ${platform.executable}`);
  }
  const executable = native.entries.find((entry) => entry.name === `package/${platform.executable}`);
  if (!executable) fail(`${native.basename} does not contain ${platform.executable}`);
  return {
    launcher,
    native,
    platform,
    payloadHash: sha256Bytes(executable.bytes),
    foreignNatives: registry.staged.filter((item) => item !== native && item.manifest.riprNative),
  };
}

// Runs the isolated registry in its own process (npm runs synchronously here)
// and returns { origin, port, staged, requests(), close() }.
async function startChildRegistry(tarballs, requestLog) {
  const staged = tarballs.map(stageTarball);
  const child = spawn(process.execPath, [path.join(__dirname, "registry.cjs"), requestLog, ...tarballs], {
    stdio: ["pipe", "pipe", "pipe"],
    windowsHide: true,
  });
  let stderr = "";
  child.stderr.on("data", (chunk) => { stderr = tail(stderr + chunk); });
  const exited = new Promise((resolve) => child.once("exit", (code, signal) => resolve({ code, signal })));
  let banner;
  try {
    banner = await new Promise((resolve, reject) => {
    let buffer = "";
    const timer = setTimeout(() => reject(new JourneyFailure("isolated registry did not start within 30s")), 30000);
    child.stdout.on("data", (chunk) => {
      buffer += chunk;
      const newline = buffer.indexOf("\n");
      if (newline !== -1) {
        clearTimeout(timer);
        try {
          resolve(JSON.parse(buffer.slice(0, newline)));
        } catch (error) {
          reject(new JourneyFailure(`isolated registry banner is not JSON: ${error.message}`));
        }
      }
    });
    exited.then(({ code }) => {
      clearTimeout(timer);
      reject(new JourneyFailure(`isolated registry exited ${code} before starting: ${stderr}`));
    });
  });
    if (!banner || !/^http:\/\/127\.0\.0\.1:\d+$/.test(banner.origin)) fail(`isolated registry must bind loopback, got ${banner && banner.origin}`);
  } catch (error) {
    // Never leave a registry process behind on a failed start.
    child.kill("SIGKILL");
    await exited;
    throw error;
  }
  return {
    origin: banner.origin,
    port: banner.port,
    staged,
    requests: () => fs.readFileSync(requestLog, "utf8").split("\n").filter(Boolean).map((line) => JSON.parse(line)),
    close: async () => {
      if (child.exitCode === null && child.signalCode === null && child.stdin && child.stdin.writable) child.stdin.end();
      const timer = setTimeout(() => child.kill("SIGKILL"), 15000);
      const result = await exited;
      clearTimeout(timer);
      return result;
    },
  };
}

class Journey {
  constructor(options) {
    this.options = options;
    this.platform = process.platform;
    this.checks = [];
    this.log = [];
    this.npm = npmCli();
    // An explicit --work must be new or empty: the run deletes it afterwards
    // and its cold-cache and decoy claims assume no earlier state.
    let base;
    if (options.work) {
      base = path.resolve(options.work);
      if (fs.existsSync(base) && fs.readdirSync(base).length > 0) fail(`--work ${base} must be a new or empty directory`);
      fs.mkdirSync(base, { recursive: true });
    } else {
      base = fs.mkdtempSync(path.join(os.tmpdir(), "ripr-npm-journey-"));
    }
    this.root = fs.realpathSync(base);
    this.home = path.join(this.root, "home");
    this.cache = path.join(this.root, "npm-cache");
    this.decoyDir = path.join(this.root, "decoy bin");
    this.sentinel = path.join(this.root, "decoy-ran");
    this.project = path.join(this.root, "consumer ü project");
    this.foreign = path.join(this.root, "foreign cwd é");
    this.controlProject = path.join(this.root, "control project");
    for (const dir of [this.home, path.join(this.home, "AppData", "Roaming"), path.join(this.home, "AppData", "Local"),
      path.join(this.root, "npm-global"), this.cache, path.join(this.root, "tmp"), this.decoyDir, this.project, this.foreign, this.controlProject]) {
      fs.mkdirSync(dir, { recursive: true });
    }
    fs.writeFileSync(path.join(this.home, ".npmrc"), "");
    fs.writeFileSync(path.join(this.root, "global-npmrc"), "");
  }

  check(name, body) {
    const started = Date.now();
    try {
      const detail = body();
      this.checks.push({ name, status: "passed", ms: Date.now() - started, detail: detail || null });
      return detail;
    } catch (error) {
      const message = error instanceof JourneyFailure ? error.message : `unexpected ${error && error.stack ? error.stack : error}`;
      this.checks.push({ name, status: "failed", ms: Date.now() - started, detail: message });
      return undefined;
    }
  }

  async checkAsync(name, body) {
    const started = Date.now();
    try {
      const detail = await body();
      this.checks.push({ name, status: "passed", ms: Date.now() - started, detail: detail || null });
      return detail;
    } catch (error) {
      const message = error instanceof JourneyFailure ? error.message : `unexpected ${error && error.stack ? error.stack : error}`;
      this.checks.push({ name, status: "failed", ms: Date.now() - started, detail: message });
      return undefined;
    }
  }

  // Child PATH: the decoy first (adversarial), then only node, git and the
  // OS system directories. Nothing that can build or download ripr.
  scrubbedPath() {
    const separator = isWindows(this.platform) ? ";" : ":";
    const git = which("git", process.env.PATH, this.platform);
    if (!git) fail("git is required for the TypeScript subject and is not on PATH");
    const system = isWindows(this.platform)
      ? [path.join(process.env.SystemRoot || "C:\\Windows", "System32"), process.env.SystemRoot || "C:\\Windows"]
      : ["/usr/bin", "/bin"];
    const dirs = [this.decoyDir, path.dirname(process.execPath), path.dirname(git), ...system];
    return [...new Set(dirs)].join(separator);
  }

  env(extra = {}) {
    const keep = isWindows(this.platform)
      ? ["SystemRoot", "SystemDrive", "ComSpec", "PATHEXT", "WINDIR", "NUMBER_OF_PROCESSORS", "PROCESSOR_ARCHITECTURE",
        "ProgramFiles", "ProgramFiles(x86)", "ProgramData"]
      : ["LANG", "LC_ALL"];
    const env = {};
    for (const key of keep) {
      if (process.env[key] !== undefined) env[key] = process.env[key];
    }
    Object.assign(env, {
      PATH: this.childPath,
      HOME: this.home,
      USERPROFILE: this.home,
      APPDATA: path.join(this.home, "AppData", "Roaming"),
      LOCALAPPDATA: path.join(this.home, "AppData", "Local"),
      npm_config_prefix: path.join(this.root, "npm-global"),
      TMPDIR: path.join(this.root, "tmp"),
      TMP: path.join(this.root, "tmp"),
      TEMP: path.join(this.root, "tmp"),
      npm_config_userconfig: path.join(this.home, ".npmrc"),
      npm_config_globalconfig: path.join(this.root, "global-npmrc"),
      npm_config_cache: this.cache,
      npm_config_registry: this.origin || "http://127.0.0.1:9/",
      npm_config_ignore_scripts: "true",
      npm_config_audit: "false",
      npm_config_fund: "false",
      npm_config_update_notifier: "false",
      npm_config_progress: "false",
      GIT_CONFIG_NOSYSTEM: "1",
      GIT_CONFIG_GLOBAL: path.join(this.root, "gitconfig"),
    }, extra);
    return env;
  }

  run(label, program, args, cwd, extraEnv) {
    const result = spawnSync(program, args, {
      cwd,
      env: this.env(extraEnv),
      encoding: "utf8",
      timeout: this.options.stepTimeoutMs || STEP_TIMEOUT_MS,
      shell: false,
      windowsHide: true,
      maxBuffer: 64 * 1024 * 1024,
      // Own a process group on POSIX so a timeout can take down every
      // descendant (npm -> launcher -> native), not just the direct child.
      detached: !isWindows(this.platform),
    });
    if (result.error && result.error.code === "ETIMEDOUT" && result.pid && !isWindows(this.platform)) {
      try {
        process.kill(-result.pid, "SIGKILL");
      } catch {
        // the group already exited
      }
    }
    const entry = {
      label,
      argv: [path.basename(program), ...args],
      cwd: path.relative(this.root, cwd) || ".",
      status: result.status,
      signal: result.signal,
      error: result.error ? String(result.error.message) : null,
      stdout: tail(result.stdout),
      stderr: tail(result.stderr),
    };
    this.log.push(entry);
    return result;
  }

  npmRun(label, args, cwd, extraEnv) {
    return this.run(label, process.execPath, [this.npm, ...args], cwd, extraEnv);
  }

  require(result, label) {
    if (result.error || result.status !== 0) {
      fail(`${label} exited ${result.status}${result.signal ? ` (${result.signal})` : ""}: ${tail(result.stderr || (result.error && result.error.message))}`);
    }
    return result;
  }

  git(args) {
    return this.require(this.run(`git ${args[0]}`, which("git", this.childPath, this.platform), [
      "-c", "user.name=ripr journey", "-c", "user.email=journey@invalid", "-c", "core.autocrlf=false",
      "-c", "init.defaultBranch=main", "-c", "commit.gpgsign=false", ...args,
    ], this.project), `git ${args[0]}`);
  }

  writeDecoy() {
    const marker = this.sentinel.replace(/\\/g, "\\\\");
    if (isWindows(this.platform)) {
      fs.writeFileSync(path.join(this.decoyDir, "ripr.cmd"), `@echo off\r\necho decoy> "${this.sentinel}"\r\necho ripr 0.0.0-decoy\r\nexit /b 0\r\n`);
    } else {
      const file = path.join(this.decoyDir, "ripr");
      fs.writeFileSync(file, `#!/bin/sh\necho decoy > '${marker}'\necho 'ripr 0.0.0-decoy'\nexit 0\n`);
      fs.chmodSync(file, 0o755);
    }
  }

  requireDecoyIdle() {
    if (fs.existsSync(this.sentinel)) fail("the ambient PATH decoy ripr was executed");
  }

  versionLine(stdout) {
    const line = String(stdout || "").replace(/\r/g, "").trim().split("\n").pop() || "";
    const expected = this.options.expectedVersionLine;
    if (expected ? line !== expected : !(line === `ripr ${this.options.version}` || line.startsWith(`ripr ${this.options.version} `))) {
      fail(`ripr reported ${JSON.stringify(line)}, expected ${JSON.stringify(expected || `ripr ${this.options.version} ...`)}`);
    }
    return line;
  }

  installedPayload(projectDir) {
    const launcherRoot = path.join(projectDir, "node_modules", ...LAUNCHER.split("/"));
    const library = path.join(launcherRoot, "lib", "launcher.cjs");
    if (!fs.existsSync(library)) fail(`installed launcher library missing at ${path.relative(this.root, library)}`);
    // The installed launcher's own resolver decides which bytes would run.
    delete require.cache[require.resolve(library)];
    const { resolveNativeExecutable } = require(library);
    const resolved = resolveNativeExecutable({ launcherRoot });
    const nativeManifest = JSON.parse(fs.readFileSync(resolved.nativeManifestPath, "utf8"));
    return {
      executable: resolved.executablePath,
      hash: sha256File(resolved.executablePath),
      nativePackage: `${nativeManifest.name}@${nativeManifest.version}`,
      launcherPackage: `${resolved.contract.name || LAUNCHER}@${resolved.contract.version}`,
    };
  }

  publicBin(projectDir) {
    return path.join(projectDir, "node_modules", ...LAUNCHER.split("/"), "bin", "ripr.cjs");
  }

  writeSubject() {
    fs.mkdirSync(path.join(this.project, "src"), { recursive: true });
    fs.mkdirSync(path.join(this.project, "test"), { recursive: true });
    fs.writeFileSync(path.join(this.project, "package.json"), `${JSON.stringify({
      name: "ripr-npm-journey-subject",
      version: "0.0.0",
      private: true,
      scripts: {
        "ripr:version": "ripr --version",
        "ripr:check": "ripr check --format json",
      },
    }, null, 2)}\n`);
    fs.writeFileSync(path.join(this.project, "ripr.toml"), "[languages]\nenabled = [\"typescript\"]\n");
    fs.writeFileSync(path.join(this.project, ".gitignore"), "node_modules/\n");
    fs.writeFileSync(path.join(this.project, "src", "pricing.ts"), SUBJECT_BASE);
    fs.writeFileSync(path.join(this.project, "test", "pricing.test.ts"), SUBJECT_TEST);
    fs.writeFileSync(path.join(this.controlProject, "package.json"), '{"name":"ripr-npm-journey-control","version":"0.0.0","private":true}\n');
  }

  async execute() {
    const options = this.options;
    const started = Date.now();
    this.childPath = "";
    this.writeDecoy();
    this.writeSubject();
    const tarballs = fs.readdirSync(options.packages)
      .filter((name) => /^effortlessmetrics-ripr(-[a-z0-9-]+)?-\d.*\.tgz$/.test(name))
      .sort()
      .map((name) => path.join(options.packages, name));
    let registry = null;
    let identities = null;
    let timings = {};

    await this.checkAsync("isolated_registry_started", async () => {
      if (tarballs.length === 0) fail(`no effortlessmetrics-ripr*.tgz tarballs in ${options.packages}`);
      registry = await startChildRegistry(tarballs, path.join(this.root, "registry-requests.jsonl"));
      this.origin = `${registry.origin}/`;
      identities = stagedIdentities(registry, options);
      return { origin: registry.origin, staged: registry.staged.map((item) => `${item.manifest.name}@${item.manifest.version}`) };
    });

    this.check("tool_identities", () => {
      const npmVersion = toolVersion(this, ["--version"]);
      if (!/^\d+\.\d+\.\d+/.test(npmVersion)) fail(`npm --version did not report a version: ${JSON.stringify(npmVersion)}`);
      this.npmVersion = npmVersion;
      return { node: process.version, npm: npmVersion, platform: `${process.platform}-${process.arch}` };
    });

    this.check("scrubbed_consumer_path", () => {
      this.childPath = this.scrubbedPath();
      for (const tool of ["cargo", "rustc", "rustup"]) {
        const found = which(tool, this.childPath, this.platform);
        if (found) fail(`${tool} is reachable from the consumer PATH at ${found}`);
      }
      const ripr = which("ripr", this.childPath, this.platform);
      if (!ripr || path.dirname(ripr) !== this.decoyDir) fail(`only the planted decoy may resolve as ripr, got ${ripr}`);
      return "cargo, rustc and rustup unreachable; ripr resolves only to the planted decoy";
    });

    this.check("subject_repository", () => {
      this.git(["init", "-q"]);
      this.git(["add", "-A"]);
      this.git(["commit", "-q", "-m", "base"]);
      fs.writeFileSync(path.join(this.project, "src", "pricing.ts"), SUBJECT_HEAD);
    });

    const ready = Boolean(registry && identities && this.childPath);
    const installed = ready && this.check("registry_install_scripts_disabled", () => {
      const before = Date.now();
      this.require(this.npmRun("npm install", [
        "install", "--ignore-scripts", "--save-dev", "--save-exact", "--no-audit", "--no-fund",
        `${LAUNCHER}@${options.version}`,
      ], this.project), "npm install");
      timings.install_ms = Date.now() - before;
      const fetched = registry.requests().filter((entry) => entry.path.startsWith("/-/tarballs/") && entry.status === 200)
        .map((entry) => decodeURIComponent(entry.path.slice("/-/tarballs/".length)));
      for (const required of [identities.launcher.basename, identities.native.basename]) {
        if (!fetched.includes(required)) fail(`npm never fetched ${required} from the isolated registry`);
      }
      const leaked = identities.foreignNatives.filter((item) => fetched.includes(item.basename));
      if (leaked.length > 0) fail(`npm fetched foreign native packages ${leaked.map((item) => item.basename).join(", ")}`);
      const lock = path.join(this.project, "package-lock.json");
      if (!fs.existsSync(lock)) fail("npm install wrote no package-lock.json");
      const lockfile = JSON.parse(fs.readFileSync(lock, "utf8"));
      const nativeKey = `node_modules/${identities.platform.package}`;
      const lockedNative = lockfile.packages && lockfile.packages[nativeKey];
      if (!lockedNative || lockedNative.version !== options.version || lockedNative.integrity !== identities.native.integrity) {
        fail(`package-lock.json does not lock ${identities.platform.package}@${options.version} to the staged integrity`);
      }
      const lockedLauncher = lockfile.packages && lockfile.packages[`node_modules/${LAUNCHER}`];
      if (!lockedLauncher || lockedLauncher.version !== options.version || lockedLauncher.integrity !== identities.launcher.integrity) {
        fail(`package-lock.json does not lock ${LAUNCHER}@${options.version} to the staged integrity`);
      }
      this.lockfileHash = sha256File(lock);
      return { fetched, lockfile: this.lockfileHash };
    }) !== undefined;

    const payload = installed ? this.check("installed_payload_identity", () => {
      const found = this.installedPayload(this.project);
      if (found.hash !== identities.payloadHash) {
        fail(`installed executable ${found.hash} differs from the staged payload ${identities.payloadHash}`);
      }
      if (found.nativePackage !== `${identities.platform.package}@${options.version}`) {
        fail(`launcher resolved ${found.nativePackage}`);
      }
      return found;
    }) : undefined;

    // Install the omit-optional control project while the registry is up so
    // later steps can prove everything else works with no registry at all.
    if (installed) {
      this.check("control_install_omit_optional", () => {
        this.require(this.npmRun("npm install --omit=optional", [
          "install", "--ignore-scripts", "--omit=optional", "--save-exact", "--no-audit", "--no-fund",
          `${LAUNCHER}@${options.version}`,
        ], this.controlProject), "npm install --omit=optional");
        const nativeDir = path.join(this.controlProject, "node_modules", ...identities.platform.package.split("/"));
        if (fs.existsSync(nativeDir)) fail("--omit=optional still installed the native package");
      });
      this.check("npm_exec_package_spec_foreign_cwd", () => {
        // The explicit one-off route installs into npm's exec cache from the
        // isolated registry only; it runs from a directory with no project.
        const result = this.require(this.npmRun("npm exec --package", [
          "exec", "--yes", `--package=${LAUNCHER}@${options.version}`, "--", "ripr", "--version",
        ], this.foreign), "npm exec --package");
        this.requireDecoyIdle();
        return this.versionLine(result.stdout);
      });
    }

    await this.checkAsync("registry_teardown", async () => {
      if (!registry) fail("registry never started");
      const exit = await registry.close();
      if (exit.code !== 0) fail(`isolated registry exited ${exit.code}${exit.signal ? ` (${exit.signal})` : ""} instead of shutting down cleanly`);
      const refused = await new Promise((resolve) => {
        const socket = net.connect({ host: "127.0.0.1", port: registry.port });
        socket.once("connect", () => { socket.destroy(); resolve(false); });
        socket.once("error", () => resolve(true));
      });
      if (!refused) fail(`registry port ${registry.port} still accepts connections after teardown`);
      const requests = registry.requests();
      const refusedRequests = requests.filter((entry) => entry.status !== 200).length;
      return { requests: requests.length, refused: refusedRequests };
    });

    if (payload) {
      this.check("project_script_version_offline", () => {
        const before = Date.now();
        const result = this.require(this.npmRun("npm run ripr:version", ["run", "--silent", "--offline", "ripr:version"], this.project), "npm run ripr:version");
        timings.launcher_ms = Date.now() - before;
        this.requireDecoyIdle();
        return this.versionLine(result.stdout);
      });

      const finding = this.check("typescript_check_nonzero_subject", () => {
        const result = this.require(this.npmRun("npm run ripr:check", ["run", "--silent", "--offline", "ripr:check"], this.project), "npm run ripr:check");
        this.requireDecoyIdle();
        let report;
        try {
          report = JSON.parse(result.stdout);
        } catch (error) {
          fail(`ripr check --format json stdout is not one JSON document: ${error.message}`);
        }
        const findings = Array.isArray(report.findings) ? report.findings : [];
        const subject = findings.find((item) => item.probe && item.probe.file === "src/pricing.ts");
        if (!subject) fail(`ripr check reported no finding on src/pricing.ts (summary ${JSON.stringify(report.summary)})`);
        const languages = report.summary && report.summary.changed_files_by_language;
        if (!Array.isArray(languages) || !languages.some((entry) => entry.language === "typescript" && entry.files > 0)) {
          fail("ripr check did not analyze the changed TypeScript file");
        }
        const route = (report.finding_routes || []).find((item) => item.finding_id === subject.id);
        if (!route || typeof route.explain_command !== "string" || !route.explain_command.startsWith("ripr explain ") ||
            !route.explain_command.includes(subject.id)) {
          fail(`ripr check emitted no explain continuation for ${subject.id}`);
        }
        return { id: subject.id, classification: subject.classification, continuation: route.explain_command };
      });

      if (finding) {
        this.check("continuation_new_process_foreign_cwd_offline", () => {
          // A fresh process tree from an unrelated directory with the registry
          // gone: npm exec must reuse the installed package, never a network.
          // Replays the exact continuation ripr printed, argument for argument.
          const argv = splitCommand(finding.continuation);
          if (argv[0] !== "ripr" || argv[1] !== "explain") fail(`continuation ${JSON.stringify(finding.continuation)} is not a ripr explain command`);
          const result = this.require(this.npmRun("npm exec <emitted continuation>", [
            "exec", "--offline", "--prefix", this.project, "--", ...argv,
          ], this.foreign), "npm exec -- <emitted continuation>");
          this.requireDecoyIdle();
          if (!result.stdout.includes("pricing.ts")) fail("ripr explain output does not name the TypeScript subject");
          return { replayed: argv };
        });
      }

      if (!isWindows(this.platform)) {
        this.check("public_bin_symlink_foreign_cwd", () => {
          const bin = path.join(this.project, "node_modules", ".bin", "ripr");
          const result = this.require(this.run("node_modules/.bin/ripr --version", bin, ["--version"], this.foreign), "node_modules/.bin/ripr");
          this.requireDecoyIdle();
          return this.versionLine(result.stdout);
        });
      }

      this.check("control_missing_optional_package", () => {
        const result = this.run("missing optional package", process.execPath, [this.publicBin(this.controlProject), "--version"], this.foreign);
        this.requireDecoyIdle();
        const expected = `cannot resolve required native package ${identities.platform.package}@${options.version}`;
        const advice = `reinstall ${LAUNCHER}@${options.version} with optional dependencies enabled`;
        if (result.status === 0) fail("the launcher succeeded without its native package");
        if (result.stdout.trim() !== "") fail("a failed launch wrote to stdout");
        if (!result.stderr.includes(expected) || !result.stderr.includes(advice)) {
          fail(`missing-package recovery did not name the exact package and reinstall advice: ${tail(result.stderr)}`);
        }
        return result.stderr.trim().split("\n")[0];
      });

      this.check("control_wrong_native_version", () => {
        this.require(this.npmRun("npm install (control, offline)", ["install", "--offline", "--ignore-scripts", "--include=optional", "--no-audit", "--no-fund"], this.controlProject), "control npm install --offline");
        const manifestPath = path.join(this.controlProject, "node_modules", ...identities.platform.package.split("/"), "package.json");
        const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
        manifest.version = `${options.version}-wrong.0`;
        fs.writeFileSync(manifestPath, JSON.stringify(manifest));
        const result = this.run("wrong native version", process.execPath, [this.publicBin(this.controlProject), "--version"], this.foreign);
        this.requireDecoyIdle();
        if (result.status === 0) fail("the launcher ran a native package with the wrong version");
        const expected = `native package ${identities.platform.package} must be version ${options.version}`;
        if (!result.stderr.includes(expected)) fail(`wrong-version refusal did not name the exact requirement: ${tail(result.stderr)}`);
        return result.stderr.trim().split("\n")[0];
      });

      this.check("control_foreign_target_native", () => {
        // A native package in the right place whose metadata names another
        // target (a foreign-platform package forced into place) must be refused.
        fs.rmSync(path.join(this.controlProject, "node_modules"), { recursive: true, force: true });
        this.require(this.npmRun("npm ci (control, offline)", ["ci", "--offline", "--ignore-scripts", "--no-audit", "--no-fund"], this.controlProject), "control npm ci --offline");
        const manifestPath = path.join(this.controlProject, "node_modules", ...identities.platform.package.split("/"), "package.json");
        const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
        const foreign = options.target === "x86_64-pc-windows-msvc" ? "x86_64-unknown-linux-gnu" : "x86_64-pc-windows-msvc";
        manifest.riprNative = { ...manifest.riprNative, target: foreign };
        fs.writeFileSync(manifestPath, JSON.stringify(manifest));
        const result = this.run("foreign target native", process.execPath, [this.publicBin(this.controlProject), "--version"], this.foreign);
        this.requireDecoyIdle();
        if (result.status === 0) fail("the launcher ran a native package built for another target");
        const expected = `native package ${identities.platform.package} must target ${options.target}`;
        if (!result.stderr.includes(expected)) fail(`foreign-target refusal did not name the exact requirement: ${tail(result.stderr)}`);
        return result.stderr.trim().split("\n")[0];
      });

      this.check("reinstall_from_lockfile_offline", () => {
        fs.rmSync(path.join(this.project, "node_modules"), { recursive: true, force: true });
        this.require(this.npmRun("npm ci --offline", ["ci", "--offline", "--ignore-scripts", "--no-audit", "--no-fund"], this.project), "npm ci --offline");
        const again = this.installedPayload(this.project);
        if (again.hash !== identities.payloadHash) fail(`reinstalled executable ${again.hash} differs from ${identities.payloadHash}`);
        if (sha256File(path.join(this.project, "package-lock.json")) !== this.lockfileHash) fail("npm ci rewrote package-lock.json");
        const result = this.require(this.npmRun("npm run ripr:version (after ci)", ["run", "--silent", "--offline", "ripr:version"], this.project), "npm run after ci");
        this.requireDecoyIdle();
        return this.versionLine(result.stdout);
      });

      this.check("uninstall_removes_launcher_and_payload", () => {
        this.require(this.npmRun("npm uninstall", ["uninstall", "--offline", "--ignore-scripts", "--no-audit", "--no-fund", LAUNCHER], this.project), "npm uninstall");
        const leftovers = [
          path.join(this.project, "node_modules", ...LAUNCHER.split("/")),
          path.join(this.project, "node_modules", ...identities.platform.package.split("/")),
          path.join(this.project, "node_modules", ".bin", isWindows(this.platform) ? "ripr.cmd" : "ripr"),
        ].filter((item) => fs.existsSync(item));
        if (leftovers.length > 0) fail(`uninstall left ${leftovers.map((item) => path.relative(this.root, item)).join(", ")}`);
        const manifest = JSON.parse(fs.readFileSync(path.join(this.project, "package.json"), "utf8"));
        if (manifest.devDependencies && manifest.devDependencies[LAUNCHER]) fail("uninstall left the devDependency entry");
      });
    }

    this.check("decoy_never_executed", () => this.requireDecoyIdle());

    return { identities, payload, timings, elapsed: Date.now() - started };
  }

  cleanup() {
    if (this.options.keep) return "not_run";
    try {
      fs.rmSync(this.root, { recursive: true, force: true, maxRetries: 3 });
      return fs.existsSync(this.root) ? "failed" : "passed";
    } catch {
      return "failed";
    }
  }
}

function toolVersion(journey, args) {
  const result = spawnSync(process.execPath, [journey.npm, ...args], { encoding: "utf8", env: journey.env(), cwd: journey.root });
  return result.status === 0 ? result.stdout.trim() : "unknown";
}

function buildReceipt(options, journey, outcome, cleanupStatus) {
  const { identities } = outcome;
  const failed = journey.checks.filter((item) => item.status !== "passed");
  const passed = journey.checks.length - failed.length;
  const zeroHash = `sha256:${"0".repeat(64)}`;
  const packageHash = identities ? identities.launcher.sha256 : zeroHash;
  const payloadHash = identities ? identities.payloadHash : zeroHash;
  const steps = journey.checks.map((item) => item.name);
  const rowPassed = failed.length === 0 && outcome.payload !== undefined;
  const row = {
    channel: "npm",
    target: options.target,
    status: rowPassed ? "passed" : "failed",
    subject_count: rowPassed ? passed : 0,
    package_hash: packageHash,
    payload_hash: payloadHash,
    executed_payload_hash: outcome.payload ? outcome.payload.hash : null,
    steps,
    limitations: failed.map((item) => `${item.name}: ${String(item.detail).slice(0, 400)}`),
    non_claims: [],
  };
  return {
    schema_version: "1.0",
    kind: "package_qualification_receipt",
    source: { commit: options.sourceCommit || "0".repeat(40), tree: options.sourceTree || "0".repeat(40) },
    release_identity: { product: "ripr", version: options.version },
    package: { channel: "npm", name: LAUNCHER, hash: packageHash },
    payload: { target: options.target, hash: payloadHash },
    tools: {
      runtime: `node ${process.version} ${process.platform}-${process.arch}`,
      package_manager: `npm ${journey.npmVersion || "unknown"}`,
    },
    subjects: { selected: journey.checks.length, executed: journey.checks.length, failed: failed.length, skipped: 0 },
    selection_scope: "explicit_subset",
    required_rows: [{ channel: "npm", target: options.target }],
    rows: [row],
    executed_steps: steps,
    elapsed: {
      install_ms: outcome.timings && outcome.timings.install_ms !== undefined ? outcome.timings.install_ms : null,
      install_condition: "cold private npm cache from the isolated loopback registry",
      launcher_ms: outcome.timings && outcome.timings.launcher_ms !== undefined ? outcome.timings.launcher_ms : null,
    },
    cleanup: { status: cleanupStatus },
    limitations: [
      "npm is the only package manager exercised; pnpm, yarn and Plug'n'Play are not claimed",
      "upgrade between two released versions is not exercised; one exact version family is staged",
      `exercised only with node ${process.version} and npm ${journey.npmVersion || "unknown"}; other Node or npm versions are not claimed`,
      "executed_payload_hash is the payload the installed launcher's own resolver selects; the TypeScript check is what shows those bytes behave as ripr",
      ...(process.platform === "win32" ? ["the node_modules/.bin/ripr.cmd shim is exercised only through npm run and npm exec"] : []),
      ...(identities ? [
        `lockfile ${journey.lockfileHash || "not written"}`,
        `native package ${identities.native.manifest.name}@${identities.native.manifest.version} ${identities.native.sha256}`,
      ] : []),
    ],
    non_claims: NON_CLAIMS,
    claim_boundary: CLAIM_BOUNDARY,
  };
}

async function main(argv) {
  const options = parseArgs(argv);
  const journey = new Journey(options);
  let outcome;
  try {
    outcome = await journey.execute();
  } catch (error) {
    journey.checks.push({ name: "journey_aborted", status: "failed", ms: 0, detail: String(error && error.stack ? error.stack : error) });
    outcome = { identities: null, payload: undefined, timings: {} };
  }
  const receipt = buildReceipt(options, journey, outcome, "passed");
  const log = { checks: journey.checks, commands: journey.log };
  const cleanupStatus = journey.cleanup();
  receipt.cleanup.status = cleanupStatus;
  fs.mkdirSync(path.dirname(path.resolve(options.receipt)), { recursive: true });
  fs.writeFileSync(options.receipt, `${JSON.stringify(receipt, null, 2)}\n`);
  if (options.log) fs.writeFileSync(options.log, `${JSON.stringify(log, null, 2)}\n`);
  for (const item of journey.checks) {
    process.stderr.write(`${item.status === "passed" ? "ok  " : "FAIL"} ${item.name}${item.status === "passed" ? "" : `: ${item.detail}`}\n`);
  }
  return receipt.rows[0].status === "passed" && cleanupStatus === "passed" ? 0 : 1;
}

module.exports = { Journey, JourneyFailure, buildReceipt, main, npmCliPath: npmCli, parseArgs, splitCommand, stagedIdentities, which };
