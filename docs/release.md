# Robinne 发布流程

## 版本规则

- `Cargo.toml` 使用纯语义化版本号，例如 `0.1.0`
- Git tag 和 GitHub Release 使用带 `v` 前缀的版本号，例如 `v0.1.0`
- 首个可分发版本固定为 `v0.1.0`
- 后续补丁版本使用 `v0.1.1`、`v0.1.2`
- 功能明显增强或发布方式升级后再进入 `v0.2.0`

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
- Windows 提供 `robinne.exe` 的 ZIP 压缩包
- 自动生成 `SHA256SUMS.txt`
- Release 默认创建为 Draft

当前版本不包含以下内容：

- macOS codesign / notarization
- Windows 签名
- DMG / MSI / NSIS / Inno Setup

## 后续发布规划

- [ ] 支持按架构分别构建和发布产物
- [ ] macOS 同时提供 `arm64` 和 `x86_64` 版本，或评估合并为 Universal Binary
- [ ] Windows 先补 `x86_64` 明确命名产物，后续再评估 `arm64`
- [ ] 在 GitHub Actions 中将 `os + target` 扩展为矩阵构建
- [ ] 统一产物命名，例如 `Robinne-v0.2.0-macos-arm64.zip`

## 发版步骤

1. 确认当前代码已经合并到 `main`
2. 确认 `Cargo.toml` 中 `version` 已更新为目标版本，例如 `0.1.0`
3. 本地执行验证：

```bash
cargo test --quiet
cargo build --release
./build_mac_app.sh --version 0.1.0
```

4. 创建标签：

```bash
git tag v0.1.0
git push origin v0.1.0
```

5. 等待 GitHub Actions 中的 `Release` 工作流完成
6. 打开 GitHub Draft Release，确认附件已生成：
   - `Robinne-v0.1.0-macos.zip`
   - `Robinne-v0.1.0-windows.zip`
   - `SHA256SUMS.txt`
7. 分别下载并验证 macOS 和 Windows 产物可以启动
8. 确认 release notes 后，手动点击 `Publish release`

## 手动补发

- 如果标签已经存在，但需要重新生成附件，可在 GitHub Actions 页面手动运行 `Release` 工作流
- 手动触发时填入目标标签，例如 `v0.1.0`
- 工作流会重新校验标签与 `Cargo.toml` 中版本是否一致

## 已知提示

- macOS 首次打开未签名应用时，系统可能提示无法验证开发者，需要手动放行
- Windows 首次运行未签名应用时，SmartScreen 可能提示未知发布者
- 这属于当前阶段的预期行为，正式对外分发前再补签名和安装器
