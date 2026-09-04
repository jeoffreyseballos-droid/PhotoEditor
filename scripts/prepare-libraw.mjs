import { mkdir, readFile, writeFile, access, copyFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const dir = path.join(root, ".tools", "native-src");
const archive = path.join(dir, "LibRaw-0.22.2.tar.gz");
const digest =
  "de86b035655accff8d4010f1a221fdf50d353cb7b1422ba26f14a0db92612cfa";
await mkdir(dir, { recursive: true });
try {
  await access(archive);
} catch {
  const response = await fetch(
    "https://www.libraw.org/data/LibRaw-0.22.2.tar.gz",
  );
  if (!response.ok)
    throw new Error(`LibRaw download failed: ${response.status}`);
  await writeFile(archive, Buffer.from(await response.arrayBuffer()));
}
if (
  createHash("sha256")
    .update(await readFile(archive))
    .digest("hex") !== digest
)
  throw new Error("LibRaw source checksum mismatch; refusing to build.");
try {
  await access(path.join(dir, "LibRaw-0.22.2", "libraw", "libraw.h"));
} catch {
  execFileSync("tar", ["-xf", archive, "-C", dir], {
    cwd: root,
    stdio: "inherit",
  });
}
const resources = path.join(root, ".resources", "raw");
await mkdir(resources, { recursive: true });
// LibRaw's LGPL-2.1/CDDL dual licensing: distribute unmodified corresponding source.
await copyFile(archive, path.join(resources, "LibRaw-0.22.2.tar.gz"));
for (const name of ["LICENSE.LGPL", "LICENSE.CDDL", "COPYRIGHT"])
  await copyFile(
    path.join(dir, "LibRaw-0.22.2", name),
    path.join(resources, name),
  );
if (!process.argv.includes("--source-only")) {
  execFileSync(
    "cargo",
    ["build", "-p", "photo-raw-helper", "--release", "--locked"],
    { cwd: root, stdio: "inherit" },
  );
  const binary =
    process.platform === "win32" ? "photo-raw-helper.exe" : "photo-raw-helper";
  await copyFile(
    path.join(root, "target", "release", binary),
    path.join(resources, binary),
  );
}
console.log("Prepared pinned LibRaw 0.22.2 inside PhotoEditor.");
