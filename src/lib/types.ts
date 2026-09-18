// SoloStack 前端共享类型定义
import type { Component } from "svelte";

export type Status = "running" | "stopped" | "partial" | "error" | "not_installed";

/** 已完整适配前端界面的组件。新增组件时必须同步扩展此联合类型。 */
export type SupportedComponent = "hadoop" | "kafka";

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

export interface EnvironmentComponentInfo {
  component: string;
  version: string;
}

export interface EnvironmentInfo {
  id: string;
  name: string;
  active: boolean;
  components: EnvironmentComponentInfo[];
  created_at: string;
  last_activated_at: string | null;
}

export interface EnvironmentUsage {
  components_bytes: number;
  data_bytes: number;
  log_bytes: number;
  runtime_bytes: number;
  other_bytes: number;
  total_bytes: number;
}

export interface EnvironmentComponentOverview {
  component: string;
  version: string;
  display_name: string;
  installed_at: string;
  status: string;
}

export interface EnvironmentOverview {
  id: string;
  name: string;
  active: boolean;
  path: string;
  components: EnvironmentComponentOverview[];
  usage: EnvironmentUsage;
  created_at: string;
  last_activated_at: string | null;
}

export interface UiComponent extends ComponentInfo {
  status: Status;
  statusText: string;
}

export interface ConfigProperty {
  name: string;
  value: string;
}

export interface ConfigFieldUpdate {
  id: string;
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
  show_logs_button: boolean;
  log_level: string;
  log_retention_days: number;
  log_max_total_mb: number;
}

export type LogLevel = "error" | "warn" | "info" | "debug" | "trace";

export interface LogRecord {
  timestamp: string;
  trace_id?: string;
  level: LogLevel;
  environment_id?: string;
  message: string;
}

export interface LogPage {
  date: string;
  records: LogRecord[];
  total: number;
  truncated: boolean;
  environments: string[];
}

export interface StreamLogLine {
  timestamp: string | null;
  trace_id?: string;
  level: LogLevel;
  environment_id?: string;
  message: string;
}

export interface LogStreamBatch {
  stream_id: string;
  sequence: number;
  records: StreamLogLine[];
  dropped: number;
}

export type LogStreamState = "starting" | "following" | "paused" | "stopped" | "error";

export interface LogStreamStatus {
  stream_id: string;
  state: LogStreamState;
  offset: number;
  dropped: number;
  rotated: boolean;
  error: string | null;
}

export interface StartLogStreamResponse {
  stream_id: string;
  records: StreamLogLine[];
  offset: number;
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

export interface LogoProps {
  class?: string;
}

export interface InstallFieldsProps {
  component: SupportedComponent;
  version: string;
  params: Record<string, string>;
  onParamChange: (id: string, value: string) => void;
}

export interface ConfigFieldsProps {
  component: SupportedComponent;
  version: string;
  values: Record<string, string>;
  jdks: JdkInfo[];
  onFieldChange: (id: string, value: string) => void;
}

/** 组件专属 UI 与后端字段契约的唯一前端入口。 */
export interface ComponentAdapter {
  id: SupportedComponent;
  order: number;
  displayName: string;
  logo: Component<LogoProps>;
  installFields: Component<InstallFieldsProps>;
  configFields: Component<ConfigFieldsProps>;
  expectedInstallParamIds: (version: string) => readonly string[];
  expectedConfigFieldIds: (version: string) => readonly string[];
}
