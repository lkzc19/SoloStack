// 环境列表的加载与增删改（切换在 main：它要连带重载组件）。

import { invoke } from "@tauri-apps/api/core";
import { store } from "./state.svelte.ts";
import type { EnvironmentInfo } from "../types";

export async function loadEnvironments() {
  store.environments = await invoke<EnvironmentInfo[]>("list_environments");
  const active = store.environments.find((environment) => environment.active);
  store.activeEnvironmentId = active?.id ?? "";
  store.activeEnvironmentName = active?.name ?? "";
}

export async function createEnvironment(name: string) {
  await invoke("create_environment", { name });
  await loadEnvironments();
}

export async function renameEnvironment(id: string, name: string) {
  await invoke("rename_environment", { id, name });
  await loadEnvironments();
}

export async function deleteEnvironment(id: string) {
  await invoke("delete_environment", { id });
  await loadEnvironments();
}
