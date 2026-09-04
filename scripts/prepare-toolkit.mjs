import {
  mkdir,
  readFile,
  writeFile,
  access,
  copyFile,
  readdir,
  cp,
} from "node:fs/promises";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const downloads = path.join(root, ".tools/native-src/phase21");
const resources = path.join(root, ".resources/toolkit");
await mkdir(downloads, { recursive: true });
await mkdir(resources, { recursive: true });
async function fetchPinned(name, url, hash) {
  const target = path.join(downloads, name);
  try {
    await access(target);
  } catch {
    const response = await fetch(url);
    if (!response.ok) throw new Error(`${name}: HTTP ${response.status}`);
    await writeFile(target, Buffer.from(await response.arrayBuffer()));
  }
  if (
    createHash("sha256")
      .update(await readFile(target))
      .digest("hex") !== hash
  )
    throw new Error(`${name}: SHA-256 mismatch`);
  return target;
}
const revision = "fa2fa546052fba4c08921230a26cc69a333fca12";
const model = await fetchPinned(
  "modnet.onnx",
  `https://huggingface.co/Xenova/modnet/resolve/${revision}/onnx/model.onnx`,
  "07c308cf0fc7e6e8b2065a12ed7fc07e1de8febb7dc7839d7b7f15dd66584df9",
);
await copyFile(model, path.join(resources, "modnet.onnx"));
const license = await fetchPinned(
  "MODNet-Apache-2.0.txt",
  "https://raw.githubusercontent.com/ZHKKKe/MODNet/master/LICENSE",
  "c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4",
);
await copyFile(license, path.join(resources, "MODNet-Apache-2.0.txt"));
const windows = process.platform === "win32";
if (
  !(windows && process.arch === "x64") &&
  !(process.platform === "darwin" && process.arch === "arm64")
)
  throw new Error("Toolkit targets Windows x64 and Apple Silicon only");
const archiveName = windows
  ? "onnxruntime-win-x64-1.29.0.zip"
  : "onnxruntime-osx-arm64-1.29.0.tgz";
const runtime = await fetchPinned(
  archiveName,
  `https://github.com/microsoft/onnxruntime/releases/download/v1.29.0/${archiveName}`,
  windows
    ? "c9b4b7086b529ad814f428c1bad028e20a25d7dc0699836775faace4ab5b78b2"
    : "d0706fc34f315d8c88639d0a8c81f2e09e815f282cabed3493c06a054352cf92",
);
const runtimeDir = path.join(
  downloads,
  windows ? "onnxruntime-win-x64-1.29.0" : "onnxruntime-osx-arm64-1.29.0",
);
try {
  await access(runtimeDir);
} catch {
  execFileSync("tar", ["-xf", runtime, "-C", downloads], {
    cwd: root,
    stdio: "inherit",
  });
}
for (const file of await readdir(path.join(runtimeDir, "lib"))) {
  if (file.endsWith(".dll") || file.endsWith(".dylib"))
    await copyFile(
      path.join(runtimeDir, "lib", file),
      path.join(resources, file),
    );
}
for (const name of ["LICENSE", "ThirdPartyNotices.txt"]) {
  await copyFile(
    path.join(runtimeDir, name),
    path.join(resources, `ONNX-${name}`),
  );
}
const lensRevision = "23e8cb8050d680c7a293edb3d48b600754665f05";
const lens = await fetchPinned(
  "lensfun.tar.gz",
  `https://github.com/lensfun/lensfun/archive/${lensRevision}.tar.gz`,
  "f61b8ee4ce418b534a3fcc6ff5c6285c0a4aadc4ea2d285681a0a971aa5509f4",
);
const lensDir = path.join(downloads, `lensfun-${lensRevision}`);
try {
  await access(lensDir);
} catch {
  execFileSync("tar", ["-xf", lens, "-C", downloads], {
    cwd: root,
    stdio: "inherit",
  });
}
await cp(path.join(lensDir, "data/db"), path.join(resources, "lensfun-db"), {
  recursive: true,
});
await copyFile(
  path.join(lensDir, "data/COPYING.CC_BY-SA_3.0"),
  path.join(resources, "Lensfun-CC-BY-SA-3.0.txt"),
);
await copyFile(
  path.join(root, "docs/MODEL-NOTICES.md"),
  path.join(resources, "MODEL-NOTICES.md"),
);
if (!process.argv.includes("--assets-only")) {
  execFileSync(
    "cargo",
    ["build", "-p", "photo-mask-helper", "--release", "--locked"],
    { cwd: root, stdio: "inherit" },
  );
  const binary = windows ? "photo-mask-helper.exe" : "photo-mask-helper";
  await copyFile(
    path.join(root, "target/release", binary),
    path.join(resources, binary),
  );
}
console.log(
  "Prepared pinned local MODNet, CPU ONNX Runtime and Lensfun database.",
);
