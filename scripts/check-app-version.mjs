#!/usr/bin/env node

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const packageJsonPath = resolve(root, "package.json");
const tauriConfigPath = resolve(root, "src-tauri/tauri.conf.json");

const packageJson = JSON.parse(readFileSync(packageJsonPath, "utf8"));
const tauriConfig = JSON.parse(readFileSync(tauriConfigPath, "utf8"));

if (tauriConfig.version !== "../package.json") {
  throw new Error(
    `src-tauri/tauri.conf.json 的 version 必须指向 ../package.json，当前为 ${JSON.stringify(tauriConfig.version)}`,
  );
}

const expectedTag = `v${packageJson.version}`;
const requestedTag = process.argv[2]?.trim();
if (requestedTag && requestedTag !== expectedTag) {
  throw new Error(
    `发布 tag 与 package.json 版本不一致: tag=${requestedTag}, expected=${expectedTag}`,
  );
}

console.log(packageJson.version);
