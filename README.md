# GDOU Net Login

广东海洋大学校园网 Srun 自动登录器。当前版本是 Rust 原生桌面应用，支持 Windows、macOS Intel 和 macOS Apple Silicon。

## 功能

- 双击启动图形界面
- 保存账号、门户地址、检测间隔和 `ac_id`
- 密码保存到系统钥匙串
- 手动登录、登出、检测在线状态
- 窗口保持打开时自动检测断线并重连
- GitHub Actions 远程构建 Windows 和 macOS 版本

## 普通用户使用

1. 从 GitHub Actions 或 Release 下载对应系统的压缩包。
2. Windows 解压后双击 `gdou-net-login.exe`。
3. macOS 解压后运行 `gdou-net-login`。如果系统提示阻止打开，需要在“系统设置 - 隐私与安全性”里允许。
4. 在窗口里填写账号和密码。
5. `Portal URL` 默认是 `http://10.129.1.1`，一般不用改。
6. 勾选 `Auto reconnect`，保持窗口打开，断网后会按检测间隔自动重连。

首次保存时，系统可能会弹出钥匙串/凭据管理器授权窗口，这是为了保存密码。

## 开发者使用

安装 Rust 后：

```bash
cd desktop
cargo run
```

构建 Windows 本机 exe：

```bash
cd desktop
cargo build --release
```

生成的文件在：

```text
desktop/target/release/gdou-net-login.exe
```

命令行调试仍然保留：

```bash
gdou-net-login init
gdou-net-login login
gdou-net-login logout
gdou-net-login status
gdou-net-login watch --interval 30
gdou-net-login tray
```

## 远程构建

仓库包含 `.github/workflows/desktop-release.yml`。可以在 GitHub Actions 手动运行，也可以推送 tag：

```bash
git tag desktop-v0.1.0
git push origin desktop-v0.1.0
```

构建产物：

- `gdou-net-login-windows-x86_64.zip`
- `gdou-net-login-macos-x86_64.zip`
- `gdou-net-login-macos-aarch64.zip`

## 界面模板

当前 GUI 使用 `egui` 原生控件，优先保证轻量、单文件、跨平台。如果后续想做更精致的网页风格界面，可以迁移到 Tauri + React/Vite，再参考：

- [shadcn/ui Blocks](https://ui.shadcn.com/blocks)
- [Tailwind UI Components](https://tailwindui.com/components)
- [DaisyUI Components](https://daisyui.com/components)
- [Flowbite Login Blocks](https://flowbite.com/blocks/marketing/login/)
- [Figma Community](https://www.figma.com/community)
