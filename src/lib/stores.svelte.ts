// SoloStack 前端共享状态与操作的公开入口。
//
// 实现按域拆在 `lib/stores/` 下，这里只做 re-export，调用方仍用 `$lib/stores.svelte.ts`：
//
// | 文件 | 职责 |
// |---|---|
// | `stores/state.svelte.ts` | `store` 状态对象 + 纯取值/格式化（状态文案、单位、路径） |
// | `stores/theme.svelte.ts` | 浅色 / 深色 / 跟随系统 |
// | `stores/environment.svelte.ts` | 环境列表加载与增删改 |
// | `stores/component.svelte.ts` | 组件列表 / 状态刷新、启停 |
// | `stores/install.svelte.ts` | 安装进度、安装 / 取消 / 完成、卸载 |
// | `stores/main.svelte.ts` | 启动、主数据加载、环境切换（编排上面两组） |

export * from "./stores/state.svelte.ts";
export * from "./stores/theme.svelte.ts";
export * from "./stores/environment.svelte.ts";
export * from "./stores/component.svelte.ts";
export * from "./stores/install.svelte.ts";
export * from "./stores/main.svelte.ts";
