import { cp, mkdir, access } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const packageName =
  process.platform === "win32"
    ? "exiftool-vendored.exe"
    : "exiftool-vendored.pl";
const source = path.join(root, "node_modules", packageName);
const destination = path.join(root, ".resources", "exiftool");
await access(path.join(source, "LICENSE"));
await mkdir(destination, { recursive: true });
await cp(path.join(source, "bin"), path.join(destination, "bin"), {
  recursive: true,
  force: true,
  preserveTimestamps: true,
});
await cp(path.join(source, "LICENSE"), path.join(destination, "LICENSE"));
await cp(
  path.join(source, "package.json"),
  path.join(destination, "package.json"),
);
console.log(`Prepared bundled ExifTool (${packageName}) inside PhotoEditor.`);
