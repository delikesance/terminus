import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const tsc = path.join(root, "node_modules/typescript/bin/tsc");
const r = spawnSync(
  process.execPath,
  [
    tsc,
    "--module",
    "esnext",
    "--target",
    "es2022",
    "--moduleResolution",
    "bundler",
    "--outDir",
    "src",
    "src/portableBundle.ts",
  ],
  { cwd: root, stdio: "inherit" },
);
if (r.status !== 0) {
  process.exit(r.status ?? 1);
}
