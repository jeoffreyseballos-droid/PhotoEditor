// Copy portable trained-style packages into the desktop resource tree.
import { cp, mkdir, rm } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const source = path.join(root, "styles");
const destination = path.join(root, ".resources/styles");
await mkdir(path.dirname(destination), { recursive: true });
await rm(destination, { recursive: true, force: true });
await cp(source, destination, { recursive: true, force: false });
console.log("Prepared validated portable trained-style packages.");
