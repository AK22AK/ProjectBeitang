# Robinne 发布流程

## 推荐开发流程

从现在开始，推荐使用 `功能分支 -> PR -> main -> tag release` 这条路径，而不是直接把功能提交推到 `main`。

日常开发流程：

1. 从 `main` 拉最新代码。
2. 新建功能分支，例如 `codex/settings-shortcut-fix`。
3. 在本地开发并完成基础自测。
4. 推送功能分支并创建 PR，目标分支为 `main`。
5. 等待 GitHub Actions 常规 CI 通过：
   - `Test macOS`
   - `Test Windows`
6. CI 全绿后再合并到 `main`。
7. 版本号变更和 release note 也建议走单独 PR。
8. 只有在 `main` 最新提交已经通过常规 CI 后，才打 `v*.*.*` tag 触发正式发布。

如果仍然直接 `push main`：

- 常规 CI 依然会在远程自动执行。
- 但它只能在 push 之后告诉你结果，不能在进入 `main` 之前拦住问题。
- 因此这种模式不再推荐作为默认流程。

## GitHub 仓库门禁设置

为了让 PR 流程真正生效，需要在 GitHub 仓库中对 `main` 开启 branch protection。

建议至少勾选以下规则：

- `Require a pull request before merging`
- `Require status checks to pass before merging`
- 将以下检查设为 required：
  - `Test macOS`
  - `Test Windows`

可选但推荐：

- `Require branches to be up to date before merging`
- 禁止直接 push 到 `main`

这样之后：

- 功能代码必须先经过三平台 CI
- Windows 构建或测试问题会在合并前暴露
- Release workflow 不再承担“第一次发现跨平台问题”的职责

## 版本规则

- `Cargo.toml` 使用纯语义化版本号，例如 `0.2.0`
- Git tag 和 GitHub Release 使用带 `v` 前缀的版本号，例如 `v0.2.0`
- 首个可分发版本固定为 `v0.1.0`
- 后续补丁版本使用 `v0.2.1`、`v0.2.2`
- 存在对外可感知的新功能迭代时，提升次版本，例如从 `v0.1.0` 进入 `v0.2.0`

### 自动判定规则

- 存在 `BREAKING CHANGE` 或 `feat!:` 这类破坏性提交时，提升大版本
- 存在 `feat:` 提交时，提升次版本
- 只有 `fix:`、`refactor:`、`chore:`、`docs:`、`test:` 时，提升补丁版本
- 如果仓库还没有 release tag，则沿用当前 `Cargo.toml` 中的版本号作为首个发布版本

## 自动准备发布

可以直接运行：

```bash
./scripts/prepare_release.sh
```

这个脚本会自动完成以下事情：

- 找到上一个 `v*.*.*` tag
- 根据自上个 tag 以来的提交类型推断下一个版本号
- 更新 `Cargo.toml`
- 生成发布说明草稿到 `dist/release-prep/RELEASE_NOTES-v<version>.md`
- 运行本地校验：
  - `cargo test --quiet`
  - `cargo build --release`
  - macOS 下额外生成 `Robinne-v<version>-macos.zip`
- macOS 下额外生成 `Robinne-v<version>-macos.dmg`
- Windows 产物由 GitHub Actions 的 Windows runner 统一构建和打包

注意：

- `prepare_release.sh` 适合在已经通过常规 CI 的提交上使用。
- 它不能替代 PR 门禁；它只是在发版准备阶段做版本推断、产物准备和补充校验。

如果需要为某个版本提供手写 release note，可在仓库中新增：

```text
docs/releases/v<version>.md
```

当这个文件存在时，GitHub Release 工作流会优先使用它作为发布说明；不存在时，继续回退到 GitHub 自动生成的 release notes。

常用参数：

```bash
./scripts/prepare_release.sh --skip-verify
./scripts/prepare_release.sh --version 0.1.1
```

说明：

- 默认要求工作区没有未提交的 tracked 变更
- 如果确实需要在脏工作区执行，可加 `--allow-dirty`

## 本次发布范围

- 自动化双平台 GitHub Release
- macOS 提供 `Robinne.app` 的 ZIP 压缩包
- macOS 可额外生成带 `Applications` 快捷方式的 DMG 安装盘，但在完成 codesign / notarization 前不作为推荐分发方式
- Windows 提供 `robinne.exe` 的 ZIP 压缩包
- 自动生成 `SHA256SUMS.txt`
- Release 由 GitHub Actions 自动创建并直接发布
- 当前代码基线以 macOS / Windows 双平台为准，Linux 不在本轮支持范围内

当前版本不包含以下内容：

- macOS codesign / notarization
- Windows 签名
- MSI / NSIS / Inno Setup

在补齐 macOS codesign / notarization 之前：

- 对外分发优先使用 macOS ZIP 包中的 `Robinne.app`
- DMG 仅用于内部验证构建链路，不作为推荐安装方式

## 后续发布规划

- [ ] 支持按架构分别构建和发布产物
- [ ] macOS 同时提供 `arm64` 和 `x86_64` 版本，或评估合并为 Universal Binary
- [ ] Windows 先补 `x86_64` 明确命名产物，后续再评估 `arm64`
- [ ] 在 GitHub Actions 中将 `os + target` 扩展为矩阵构建
- [ ] 统一产物命名，例如 `Robinne-v0.2.0-macos-arm64.zip`
- [ ] 为 DMG 增加 codesign / notarization 与更完整的安装盘视觉样式

## 发版步骤

1. 确认目标功能已经通过 PR 合并到 `main`
2. 确认 `main` 最新提交的常规 CI 已经全绿：
   - `Test macOS`
   - `Test Windows`
3. 确认 `Cargo.toml` 中 `version` 已更新为目标版本，例如 `0.2.0`
4. 如有需要，补充 `docs/releases/v0.2.0.md`
5. 本地执行验证：

```bash
cargo test --quiet
cargo build --release
./build_mac_app.sh --version 0.2.0
# Windows 打包在 GitHub Actions 的 Windows runner 完成
```

6. 创建标签：

```bash
git tag v0.2.0
git push origin v0.2.0
```

7. 等待 GitHub Actions 中的 `Release` 工作流完成
8. 打开 GitHub Release 页面，确认附件已生成：
   - `Robinne-v0.2.0-macos.zip`
   - `Robinne-v0.2.0-macos.dmg`
   - `Robinne-v0.2.0-windows.zip`
   - `SHA256SUMS.txt`
9. 优先下载 macOS ZIP 包并验证其中的 `Robinne.app` 可以启动；DMG 仅用于内部验证
10. 确认发布说明（手写或自动生成）与附件内容正确

## 常规 CI 与 Release 的职责分工

常规 CI：

- 触发时机：
  - `pull_request`
  - `push` 到 `main`
- 执行平台：
  - macOS
  - Windows
- 主要职责：
  - `cargo test --quiet`
  - `cargo build --release`

Release workflow：

- 触发时机：
  - 推送 `v*.*.*` tag
  - 手动 `workflow_dispatch`
- 主要职责：
  - 校验 tag 与 `Cargo.toml` 版本一致
  - 双平台打包产物
  - 生成 `SHA256SUMS.txt`
  - 创建或更新 GitHub Release

职责边界：

- “代码能不能在 Windows 上编译和通过测试” 属于常规 CI
- “这份已验证代码能不能打成发布附件” 属于 Release workflow

## 手动补发

- 如果标签已经存在，但需要重新生成附件，可在 GitHub Actions 页面手动运行 `Release` 工作流
- 手动触发时填入目标标签，例如 `v0.2.0`
- 工作流会重新校验标签与 `Cargo.toml` 中版本是否一致

## 已知提示

- macOS 首次打开未签名应用时，系统可能提示无法验证开发者，需要手动放行
- 在完成 codesign / notarization 前，GitHub Release 中的 `.dmg` 不应作为默认安装入口
- 只有把 `Robinne.app` 拖入 `/Applications` 或 `~/Applications` 后，Spotlight / Launchpad 才更稳定地检索到它；仅在下载目录直接运行 `.app` 或在挂载的 `.dmg` 中直接运行，通常不会被当作已安装应用
- Windows 首次运行未签名应用时，SmartScreen 可能提示未知发布者
- 这属于当前阶段的预期行为，正式对外分发前再补签名和安装器
