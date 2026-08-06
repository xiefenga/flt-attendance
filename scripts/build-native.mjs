import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const projectRoot = path.resolve(import.meta.dirname, "..");
const result = spawnSync("cargo", ["build", "-p", "attendance-napi", "--release"], {
  cwd: projectRoot,
  stdio: "inherit"
});
if (result.status !== 0) process.exit(result.status ?? 1);

const libraryName =
  process.platform === "win32"
    ? "attendance_napi.dll"
    : process.platform === "darwin"
      ? "libattendance_napi.dylib"
      : "libattendance_napi.so";
const source = path.join(projectRoot, "target", "release", libraryName);
const outputDirectory = path.join(projectRoot, "resources", "native");
const destination = path.join(outputDirectory, "attendance.node");
fs.mkdirSync(outputDirectory, { recursive: true });
fs.copyFileSync(source, destination);
console.log(`Native module: ${destination}`);

