# 05 SoloStack 通知系统 TODO

> 状态：下一版本设计。
> 来源：日志写入失败、生命周期异常和应用更新等事件需要统一的用户可见反馈。
> 目标：同时提供系统级通知和应用内通知，不依赖用户主动打开日志页面。

---

## 1. 目标

- [ ] 建立统一的通知事件模型。
- [ ] 支持 macOS 系统级通知。
- [ ] 支持应用内通知中心。
- [ ] 支持短时 toast 和需要用户确认的通知。
- [ ] 通知可以关联环境、组件、版本、操作和 `trace_id`。
- [ ] 相同事件可以合并，避免刷屏。
- [ ] 用户可以设置通知级别和通知渠道。
- [ ] 系统通知权限被拒绝时，应用内通知仍然可用。
- [ ] 点击通知可以跳转到对应组件、环境或日志视图。

---

## 2. 首批触发场景

- [ ] 日志写入失败，例如磁盘写满或目录不可写。
- [ ] 安装、启动、停止、卸载失败。
- [ ] 环境切换失败。
- [ ] 旧数据迁移失败或需要用户处理。
- [ ] 发现新版本或更新安装失败。
- [ ] 组件运行状态异常。

---

## 3. 建议模型

```text
NotificationEvent
  id
  level
  source
  environment_id
  component
  version
  operation
  trace_id
  title
  message
  action
  created_at
  dedupe_key
```

建议模块：

```text
crates/core/src/app/notification.rs
src-tauri/src/commands/notification.rs
src/lib/notifications.svelte.ts
```

核心组件：

- [ ] `NotificationEvent`：统一定义通知内容、上下文和跳转目标。
- [ ] `NotificationHub`：去重、聚合、分发和已读状态。
- [ ] `SystemNotifier`：调用 macOS 系统通知。
- [ ] `InAppNotificationStore`：应用内通知中心状态。
- [ ] 通知设置：级别、渠道、免打扰和保留时间。

---

## 4. Tauri 与前端接口

- [ ] `list_notifications`：读取通知历史。
- [ ] `mark_notification_read`：标记已读。
- [ ] `clear_notifications`：清空应用内通知。
- [ ] `get_notification_settings`：读取通知设置。
- [ ] `set_notification_settings`：保存通知设置。
- [ ] 事件 `notification://created`：新建通知。
- [ ] 应用内通知中心。
- [ ] 顶部或侧边 toast 容器。
- [ ] 通知点击后的页面跳转。

---

## 5. 设计决策

- [ ] 通知历史是否持久化，以及持久化保留期限。
- [ ] 系统通知默认开启还是首次需要时申请权限。
- [ ] 哪些级别默认发送系统通知。
- [ ] 相同错误在时间窗口内如何合并。
- [ ] 任务成功后是否通知，还是只通知失败和需要用户处理的事件。
- [ ] 通知点击后如何定位到组件、环境或日志条目。

---

## 6. 非目标

- [ ] 不接入远程推送服务。
- [ ] 不上传通知内容或用户日志。
- [ ] 不在第一阶段支持 Windows/Linux 的系统通知差异。
- [ ] 不用通知替代结构化日志。
