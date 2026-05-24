# GDOU Net Login

广东海洋大学校园网自动登录器。Rust + Tauri 桌面程序，支持 Windows 和 macOS（Intel / Apple Silicon）。

## 功能

- 登录 / 退出校园网
- 自动检测断线并重连
- 保存账号、密码、Portal 地址和 `ac_id`

## 运行

```bash
cd desktop
cargo run
```

## 构建

```bash
cd desktop
cargo build --release
```

Windows 可执行文件：

```text
desktop/target/release/gdou-net-login.exe
```
