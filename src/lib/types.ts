// SoloStack 前端共享类型定义

export type Status = "running" | "stopped" | "partial" | "error" | "not_installed";

/** 外观主题模式：浅色 / 深色 / 跟随系统。 */
export type ThemeMode = "light" | "dark" | "system";

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
  close_to_tray: boolean;
}

export interface InstallSource {
  name: string;
  versions: Record<string, string>;
}

/** 一个安装参数的声明（id + 默认值）；布局与文案由前端按组件定制 */
export interface InstallParam {
  id: string;
  default: string;
}

export interface ManifestInfo {
  component: string;
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
  /** 组件官方配置目录（配置就地写在实例内，不再是副本） */
  config: string;
  data: string;
  log: string;
}
