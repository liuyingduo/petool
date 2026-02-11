import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const FILTERED_WARNINGS = [
  "libpng warning: tRNS: invalid with alpha channel",
];

function shouldFilter(line) {
  return FILTERED_WARNINGS.some((warning) => line.includes(warning));
}

function forwardStream(stream, writer) {
  let buffer = "";

  stream.on("data", (chunk) => {
    buffer += chunk.toString();

    while (true) {
      const newlineIndex = buffer.indexOf("\n");
      if (newlineIndex === -1) break;

      const line = buffer.slice(0, newlineIndex + 1);
      buffer = buffer.slice(newlineIndex + 1);

      if (!shouldFilter(line)) {
        writer.write(line);
      }
    }
  });

  stream.on("end", () => {
    if (buffer.length > 0 && !shouldFilter(buffer)) {
      writer.write(buffer);
    }
  });
}

function getListeningPids(port) {
  if (process.platform !== "win32") {
    return [];
  }

  const cmd = `Get-NetTCPConnection -State Listen -LocalPort ${port} -ErrorAction SilentlyContinue | Select-Object -ExpandProperty OwningProcess -Unique`;
  const result = spawnSync("powershell", ["-NoProfile", "-Command", cmd], {
    encoding: "utf8",
  });

  if (result.error) {
    return [];
  }

  return result.stdout
    .split(/\r?\n/)
    .map((line) => Number.parseInt(line.trim(), 10))
    .filter((pid) => Number.isInteger(pid) && pid > 0);
}

function killPid(pid) {
  if (process.platform !== "win32") {
    return;
  }

  spawnSync("taskkill", ["/PID", String(pid), "/T", "/F"], {
    stdio: "ignore",
  });
}

function killProcessImage(imageName) {
  if (process.platform !== "win32") {
    return;
  }

  spawnSync("taskkill", ["/IM", imageName, "/T", "/F"], {
    stdio: "ignore",
  });
}

function ensurePortFree(port) {
  const pids = getListeningPids(port);
  if (pids.length === 0) {
    return;
  }

  process.stdout.write(
    `[tauri-dev] Port ${port} is in use by PID(s): ${pids.join(", ")}. Terminating stale process(es)...\n`,
  );

  for (const pid of pids) {
    killPid(pid);
  }

  const remaining = getListeningPids(port);
  if (remaining.length > 0) {
    throw new Error(
      `Port ${port} is still in use by PID(s): ${remaining.join(", ")}. Please close them manually and retry.`,
    );
  }
}

killProcessImage("petool.exe");
ensurePortFree(5173);

function buildEnv() {
  const env = { ...process.env };
  const pathKey =
    Object.keys(env).find((key) => key.toLowerCase() === "path") || "PATH";
  const home = env.USERPROFILE || env.HOME;
  if (!home) {
    return env;
  }

  const cargoBin = path.join(home, ".cargo", "bin");
  if (!fs.existsSync(cargoBin)) {
    return env;
  }

  const sep = process.platform === "win32" ? ";" : ":";
  const currentPath = env[pathKey] || "";
  const lowerPath = currentPath.toLowerCase();
  if (!lowerPath.includes(cargoBin.toLowerCase())) {
    env[pathKey] = `${cargoBin}${sep}${currentPath}`;
  }

  return env;
}

function resolveTauriLaunch() {
  const tauriBin = path.join(
    process.cwd(),
    "node_modules",
    ".bin",
    process.platform === "win32" ? "tauri.cmd" : "tauri",
  );
  if (fs.existsSync(tauriBin)) {
    return { command: tauriBin, args: ["dev"], shell: false };
  }

  const tauriCliJs = path.join(
    process.cwd(),
    "node_modules",
    "@tauri-apps",
    "cli",
    "tauri.js",
  );
  if (fs.existsSync(tauriCliJs)) {
    return {
      command: process.execPath,
      args: [tauriCliJs, "dev"],
      shell: false,
    };
  }

  return { command: "tauri", args: ["dev"], shell: true };
}

const launch = resolveTauriLaunch();

const child = spawn(launch.command, launch.args, {
  shell: launch.shell,
  env: buildEnv(),
  stdio: ["inherit", "pipe", "pipe"],
});

forwardStream(child.stdout, process.stdout);
forwardStream(child.stderr, process.stderr);

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 1);
});
