# Robinne 平台兼容说明

## 当前支持范围

- 当前正式支持 `macOS` 和 `Windows`
- 当前不支持 `Linux`，也不承诺 Linux 可运行
- 平台差异统一收口在 [src/platform/](/Users/jiangzhengjie/Project/Robinne/src/platform)

## 当前平台差异

| 能力 | macOS | Windows | 当前策略 |
| --- | --- | --- | --- |
| 文件选择 | `NSOpenPanel` | `rfd` | 对上统一为平台文件对话框接口 |
| 文件对话框预热 | 启用 | 不启用 | 仅保留在 macOS |
| 附件预览 | Quick Look | 系统默认应用打开 | 允许体验不同，但主流程必须可用 |
| 桌面通知 | `notify-rust` + macOS 声音设置 | `notify-rust` | 统一通知接口，平台特调只留在平台层 |
| 应用内菜单 | 保留系统菜单与 `Services` | 仅保留必要菜单项 | 不要求菜单结构完全一致 |
| 快捷键展示 | `Cmd` | `Ctrl` | 展示文案和默认值按平台输出 |
| 全局快捷键 | best-effort | best-effort | 注册失败不阻塞应用启动 |
| 打包产物 | `Robinne.app` ZIP | `robinne.exe` ZIP | 不在本轮提供安装器 |

## 当前架构约束

- 业务层、数据层、绝大多数 UI 层不得直接使用平台 API
- `cfg(target_os = ...)` 应尽量只出现在：
  - [src/platform/](/Users/jiangzhengjie/Project/Robinne/src/platform)
  - [build.rs](/Users/jiangzhengjie/Project/Robinne/build.rs)
  - 必须保留的平台资源或构建脚本
- UI、入口和业务代码只调用平台 facade，例如：
  - 文件选择：`platform::pick_image_files`
  - 附件预览：`platform::open_saved_attachment`
  - 通知：`platform::send_reminder`
  - 快捷键文案：`platform::default_global_shortcuts`、`platform::app_shortcut_entries`
  - 菜单：`platform::build_app_menus`

## 新增平台相关特性时的开发规则

### 1. 先定义能力，再写平台实现

新增平台相关功能时，先在 `src/platform/` 中定义统一接口，再分别实现 `macOS` / `Windows`。

不要在 UI 点击事件、store 或业务逻辑里直接：

- 调用 `open` / `cmd /C start`
- 引入 `objc2`、AppKit、Quick Look
- 散落写 `cfg(target_os = "macos")`

### 2. 默认要求

- `macOS` 和 `Windows` 都必须保证主流程可用
- 不要求两端系统集成体验完全一致
- 如果某平台缺少原生等价能力，允许降级，但必须：
  - 有明确 fallback
  - 不影响核心流程
  - 错误文案统一且可理解

### 3. 允许的平台差异

以下差异视为允许：

- 菜单结构不同
- 快捷键主修饰键不同
- 附件预览方式不同
- 通知样式和声音不同

以下差异默认不允许：

- 某一平台无法创建/编辑任务和记录
- 某一平台无法导入导出
- 因系统集成失败导致应用无法启动
- UI 到处出现硬编码的 `macOS` 专属文案

## 无 Windows 设备时的开发与验证

当前没有 Windows 设备时，按下面的方式开发：

### 本地可做

- 在 macOS 上完成功能开发
- 把平台差异集中写在 `src/platform/`
- 运行：
  - `cargo fmt --all`
  - `cargo test --quiet`
- 如需检查 Windows target，优先做静态解析或在 CI 上验证

### 不能依赖本地结论的内容

没有 Windows 设备时，不要假设以下行为已经正确：

- 全局快捷键实际可注册
- 通知在 Windows 上实际弹出
- 系统默认应用打开行为完全符合预期
- 菜单和快捷键在 Windows UI 中展示正常

### 必须依赖 CI 或真实设备确认的内容

- `cargo test --quiet` on Windows runner
- `cargo build --release` on Windows runner
- `robinne.exe` 可启动
- 附件点击能打开系统默认应用
- 全局快捷键失败时应用仍可正常使用

## 后续维护建议

- 每新增一个平台特性，都在本文档追加一行“能力差异表”
- 如果未来支持 Linux，再单独增加一列；不要提前在业务层写 Linux 分支
- 如果某能力开始变复杂，例如“系统文件管理器交互”“窗口置顶/激活”“托盘”，应在 `src/platform/` 下新建独立模块，而不是继续堆进现有 UI 文件
