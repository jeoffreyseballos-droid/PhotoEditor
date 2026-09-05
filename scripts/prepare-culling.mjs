// Optional local face geometry; immutable upstream revision and verified model/license.
import { access, copyFile, mkdir, readFile, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
if (!(
  (process.platform === "win32" && process.arch === "x64") ||
  (process.platform === "darwin" && process.arch === "arm64")
))
  throw new Error(
    "Culling resources target Windows x64 or macOS Apple Silicon.",
  );
const downloads = path.join(root, ".tools/native-src/phase5");
const resources = path.join(root, ".resources/toolkit");
await mkdir(downloads, { recursive: true });
await mkdir(resources, { recursive: true });
const revision = "f12e12798e8314f7c074a6656816c048dcc95b7a";
async function pinned(name, url, hash) {
  const file = path.join(downloads, name);
  try {
    await access(file);
  } catch {
    const response = await fetch(url);
    if (!response.ok) throw new Error(`${name}: HTTP ${response.status}`);
    await writeFile(file, Buffer.from(await response.arrayBuffer()));
  }
  if (
    createHash("sha256")
      .update(await readFile(file))
      .digest("hex") !== hash
  )
    throw new Error(`${name}: SHA-256 mismatch`);
  await copyFile(file, path.join(resources, name));
}
await pinned(
  "yunet-2023mar.onnx",
  `https://media.githubusercontent.com/media/opencv/opencv_zoo/${revision}/models/face_detection_yunet/face_detection_yunet_2023mar.onnx`,
  "8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4",
);
await pinned(
  "YuNet-MIT.txt",
  `https://raw.githubusercontent.com/opencv/opencv_zoo/${revision}/models/face_detection_yunet/LICENSE`,
  "c83b8120c50ccbd4c4f96edf53141bdd566ebb8f8e9227e415326aa1b1aba958",
);
if (!process.argv.includes("--assets-only")) {
  execFileSync(
    "cargo",
    ["build", "-p", "photo-face-helper", "--release", "--locked"],
    { cwd: root, stdio: "inherit" },
  );
  const binary =
    process.platform === "win32"
      ? "photo-face-helper.exe"
      : "photo-face-helper";
  await copyFile(
    path.join(root, "target/release", binary),
    path.join(resources, binary),
  );
}
console.log(
  "Prepared pinned local YuNet face geometry. Eye-state inference is not supplied by YuNet.",
);
