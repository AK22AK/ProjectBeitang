# GPUI 文字不显示问题调试记录

## 问题描述
应用程序窗口能正常显示，背景色、布局都正常，但所有文字（包括标题、按钮文字、输入框内容）都不可见。

## 根本原因
**GPUI 在 macOS 平台上需要启用 `font-kit` feature 才能正常渲染文字。**

这是 GPUI 字体渲染后端的平台差异问题：
- 官方示例使用了 `gpui_platform = { features = ["font-kit"] }`
- 我们的项目最初没有这个 feature，导致字体系统无法初始化

## 修复方案

### 1. 修改 Cargo.toml
```toml
# 错误的配置（文字不显示）
gpui_platform = { git = "https://github.com/zed-industries/zed" }

# 正确的配置（文字正常显示）
gpui_platform = { git = "https://github.com/zed-industries/zed", features = ["font-kit"] }
```

### 2. 清理并重新编译
```bash
cargo clean
cargo build
```

**注意**：必须执行 `cargo clean`，因为 feature 变更需要重新编译所有依赖。

## 调试过程复盘

### 尝试过但无效的方法
1. ❌ 强制设置文字颜色为黑色/红色
2. ❌ 强制使用浅色主题
3. ❌ 使用同步方式打开窗口
4. ❌ 修改 Root 背景色设置
5. ❌ 各种布局调整

### 关键突破点
1. **对比官方示例**：发现官方示例能正常显示文字
2. **环境隔离测试**：完全相同的代码在不同目录编译结果不同
3. **依赖版本对比**：发现 gpui_platform 的 feature 配置差异

### 验证方法
```bash
# 创建最小化测试项目，完全复制官方示例配置
cd /tmp/test_gpui
cargo run
# 如果文字能显示，逐步添加我们的依赖直到找出问题
```

## 教训总结

1. **Feature 很重要**：Rust 的 feature 不仅仅是可选功能，有时是关键依赖
2. **对比官方示例**：当代码相同但行为不同时，检查依赖配置
3. **清理缓存**：修改 feature 后必须 `cargo clean`，否则缓存会导致诡异问题
4. **系统化调试**：
   - 先确认问题范围（代码 vs 环境 vs 依赖）
   - 创建最小化可复现示例
   - 逐项对比工作和不工作的配置

## 参考配置

完整的正确 Cargo.toml 依赖配置：

```toml
[dependencies]
# GPUI 框架 - 关键：必须启用 font-kit feature 才能显示文字
gpui = { git = "https://github.com/zed-industries/zed" }
gpui_platform = { git = "https://github.com/zed-industries/zed", features = ["font-kit"] }
gpui-component = { git = "https://github.com/longbridge/gpui-component" }
gpui-component-assets = { git = "https://github.com/longbridge/gpui-component" }
```

## 相关链接

- GPUI 官方仓库：https://github.com/zed-industries/zed
- gpui-component 官方仓库：https://github.com/longbridge/gpui-component
- 官方示例：https://github.com/longbridge/gpui-component/tree/main/examples/hello_world

---
记录时间：2026-03-02
调试耗时：约 4 小时
