// SoloStack 前端共享类型定义

export type Status = "running" | "stopped" | "partial" | "error" | "not_installed";

export interface WebUiUrl {
  name: string;
  url: string;
}

export interface ComponentStatusInfo {
  status: string;
}

export interface ComponentInfo {
  name: string;
  version: string;
  display_name: string;
  category: string;
  installed: boolean;
}

export interface UiComponent extends ComponentInfo {
  status: Status;
  statusText: string;
}

export interface ConfigProperty {
  name: string;
  value: string;
}

export interface LogEntry {
  path: string;
  name: string;
}

export interface SettingsInfo {
  data_root: string;
  log_viewer: string;
}

export interface InstallSource {
  name: string;
  versions: Record<string, string>;
}

export interface ComponentConfigInfo {
  id: string;
  source: InstallSource[];
  java_support: Record<string, number[]>;
}

export interface JdkInfo {
  name: string;
  vendor: string;
  version: string;
}

export interface InstallProgressPayload {
  phase: string;
  bytes: number;
  total: number;
  cached: boolean;
}

export interface DownloadPackageInfo {
  name: string;
  size: number;
}

export interface AppDef {
  name: string;
  bundle_id: string;
  icon: string;
}

export interface ComponentDirs {
  instance: string;
  etc: string;
  data: string;
  log: string;
}
