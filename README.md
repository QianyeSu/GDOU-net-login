# GDOU Net Login

广东海洋大学校园网登录与自动重连助手。  
这是一个轻量化桌面客户端，仅用于在本人已授权账号下完成校园网登录、状态检测和断线后的自动重连，主要面向科研、学习和日常远程连接场景。

> 本项目仅用于正常校园网认证登录和连接保持，不用于绕过学校网络认证、突破访问限制、共享账号、规避网络管理或进行任何违反法律法规和学校网络管理规定的行为。

## 功能

- 校园网登录、断开和状态检测
- 断线后按设定间隔自动检测并尝试重新登录
- 自动识别或手动配置 Portal 地址、`ac_id` 和客户端 IP
- 密码存入系统凭据管理，不写入本地输入缓存
- 系统托盘运行，右键可显示主窗口、检查更新或退出
- 支持 Skyborn 浅蓝、默认白色和暗色主题
- Windows 安装包和免安装 exe

## 下载与安装

当前 `0.1.0` 版本仅发布 Windows x64 安装包。

Windows 推荐下载 `GDOU Net Login_*_x64-setup.exe` 安装包。也可以使用 MSI 包或免安装 exe。

发布文件通常包括：

| 文件 | 用途 |
| --- | --- |
| `GDOU Net Login_*_x64-setup.exe` | 推荐安装包 |
| `GDOU Net Login_*_x64_en-US.msi` | MSI 安装包 |
| `gdou-net-login-windows-x86_64.exe` | 免安装直接运行 |
| `SHA256SUMS.txt` | 文件完整性校验 |

当前 Windows 安装包暂未做代码签名，首次运行时可能出现系统安全提醒。请只从本项目 Release 页面下载。

## 使用说明

1. 打开客户端。
2. 输入本人校园网账号和密码。
3. 保持“自动重连”开启。
4. 点击“登录”。
5. 最小化或关闭窗口后，程序会进入系统托盘继续运行。

如果自动识别失败，可以在“高级设置”中填入 Portal 地址、`ac_id` 或客户端 IP。

## 数据与安全

- 密码通过系统凭据管理保存。当前 Windows 版本使用 Windows Credential Manager。
- 本地输入缓存只用于恢复界面配置，不保存明文密码。
- 自动重连只会按配置间隔进行网络状态检测和正常登录请求。
- 程序不会上传账号、密码或配置到第三方服务器。

## 合规说明

使用者应遵守国家法律法规、学校网络安全管理规定和校园网使用规范。  
本软件只提供便捷登录和断线重连能力，不改变校园网认证规则，不提供破解、绕过、加速、代理或访问控制规避功能。

请仅使用本人账号，并对自己的网络行为负责。

## 本地开发

前端位于 `frontend`，桌面端位于 `desktop`。

```bash
cd frontend
npm install
npm run build
```

```bash
cd desktop
cargo run
```

## 构建

生成可直接运行的 exe：

```bash
cd desktop
../frontend/node_modules/.bin/tauri build --no-bundle
```

生成 Windows 安装包：

```bash
cd desktop
../frontend/node_modules/.bin/tauri build
```

构建产物位置：

```text
desktop/target/release/gdou-net-login.exe
desktop/target/release/bundle/nsis/
desktop/target/release/bundle/msi/
```

## 许可证

GPL-3.0-only
