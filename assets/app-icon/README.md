# App Icon 素材目录

建议把 App 图标相关文件统一放在这里，分为两类：

- `source/`: 设计源素材，放你现在准备的 PNG
- `generated/`: 从源素材导出的最终文件，例如 macOS 用的 `.icns`

推荐命名：

- `source/icon-dark.png`
- `source/icon-light.png`
- `source/icon-transparent.png`

说明：

- 这三张 PNG 可以作为源素材使用，没有问题。
- macOS 应用最终通常不会直接吃这三张 PNG，而是会从其中一张主稿导出成 `AppIcon.icns`。
- 如果后面接入打包脚本，建议把最终产物放到 `generated/AppIcon.icns`。

当前约定：

- 正式应用图标使用 `generated/AppIcon.icns`
- `generated/AppIcon-1024.png` 是当前正式图标母版
- `generated/robinne_icon_transparent_centered_1024.png` 是裁剪并居中的透明版母稿
- 生成命令：`./scripts/generate_app_icon.sh`
