import type { ComponentAdapter, SupportedComponent } from "$lib/types";
import { hadoopAdapter } from "./hadoop";
import { kafkaAdapter } from "./kafka";

const adapters = {
  hadoop: hadoopAdapter,
  kafka: kafkaAdapter,
} satisfies Record<SupportedComponent, ComponentAdapter>;

export function componentAdapter(name: string): ComponentAdapter | undefined {
  return adapters[name as SupportedComponent];
}

export function supportedComponentIds(): SupportedComponent[] {
  return Object.values(adapters)
    .sort((a, b) => a.order - b.order)
    .map((adapter) => adapter.id);
}

export function validateComponentRegistry(components: readonly { component: string }[]) {
  assertSameIds(
    "组件支持矩阵",
    supportedComponentIds(),
    components.map((component) => component.component),
  );
}

export function validateInstalledAdapters(components: readonly { name: string }[]) {
  for (const component of components) {
    if (!componentAdapter(component.name)) {
      throw new Error(`应用组件缺少前端适配: ${component.name}`);
    }
  }
}

export function validateInstallParamIds(
  component: string,
  version: string,
  actualIds: readonly string[],
) {
  const adapter = requiredAdapter(component);
  assertSameIds(
    `${adapter.displayName} v${version} 安装参数`,
    adapter.expectedInstallParamIds(version),
    actualIds,
  );
}

export function validateConfigFieldIds(
  component: string,
  version: string,
  actualIds: readonly string[],
) {
  const adapter = requiredAdapter(component);
  assertSameIds(
    `${adapter.displayName} v${version} 配置字段`,
    adapter.expectedConfigFieldIds(version),
    actualIds.filter((id) => id !== "jdk_version"),
  );
}

function requiredAdapter(component: string): ComponentAdapter {
  const adapter = componentAdapter(component);
  if (!adapter) {
    throw new Error(`应用组件缺少前端适配: ${component}`);
  }
  return adapter;
}

function assertSameIds(label: string, expected: readonly string[], actual: readonly string[]) {
  const expectedSet = new Set(expected);
  const actualSet = new Set(actual);
  const missing = [...expectedSet].filter((id) => !actualSet.has(id));
  const extra = [...actualSet].filter((id) => !expectedSet.has(id));
  if (missing.length > 0 || extra.length > 0) {
    throw new Error(
      `${label}不一致；缺少 [${missing.join(", ")}]，多出 [${extra.join(", ")}]`,
    );
  }
}
