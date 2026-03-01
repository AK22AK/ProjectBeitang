# 北堂 (Beitang)

一个使用 GPUI 框架开发的桌面任务/笔记管理应用。

## 功能特性

- 📝 快速任务捕获（支持 `!!` 高优先级、`!` 普通优先级语法）
- 📊 任务管理（创建、完成、优先级标记）
- 🗂️ 多面板导航（任务、记录、时间线、AI）
- 💾 本地 SQLite 数据存储

## 技术栈

- **UI 框架**: [GPUI](https://github.com/zed-industries/zed) (Zed 编辑器同款)
- **组件库**: [gpui-component](https://github.com/longbridge/gpui-component)
- **数据库**: SQLite (rusqlite)
- **异步运行时**: Tokio

## 重要依赖配置

⚠️ **关键**：在 macOS 上必须启用 `font-kit` feature，否则文字无法显示！

```toml
[dependencies]
gpui = { git = "https://github.com/zed-industries/zed" }
gpui_platform = { git = "https://github.com/zed-industries/zed", features = ["font-kit"] }  # ← 必须有 font-kit
gpui-component = { git = "https://github.com/longbridge/gpui-component" }
```

详见 [docs/DEBUGGING_TEXT_RENDERING.md](docs/DEBUGGING_TEXT_RENDERING.md)

## 快速开始

```bash
# 克隆项目
git clone <repository-url>
cd ProjectBeitang

# 编译运行
cargo run
```

## 使用说明

### 创建任务

在输入框中输入：
- `!! 任务内容` → 高优先级任务（红色）
- `! 任务内容` → 普通优先级任务（黄色）
- `任务内容` → 低优先级任务（绿色）

### 完成任务

点击任务左侧的复选框标记完成。

### 切换面板

点击左侧边栏的选项切换不同功能面板。

## 项目结构

```
ProjectBeitang/
├── src/
│   ├── main.rs          # 应用入口
│   ├── models.rs        # 数据模型
│   ├── store.rs         # 状态管理
│   ├── db.rs            # 数据库操作
│   └── ui/
│       ├── mod.rs
│       ├── sidebar.rs   # 侧边栏
│       └── task_panel.rs # 任务面板
├── docs/
│   └── DEBUGGING_TEXT_RENDERING.md  # 调试记录
└── Cargo.toml
```

## 开发记录

- [docs/DEBUGGING_TEXT_RENDERING.md](docs/DEBUGGING_TEXT_RENDERING.md) - GPUI 文字渲染问题调试过程

## 许可证

MIT
