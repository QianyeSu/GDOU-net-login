# GDOU Net Login

广东海洋大学校园网自动登录与断线重连桌面客户端，适合需要长期保持电脑在线、远程连接实验室电脑，或避免网络断开后手动重新认证的日常场景。当前版本面向 Windows。

> 本项目只用于正常校园网认证登录和连接保持。请仅使用本人已授权账号，并遵守学校网络管理规定。

![GDOU Net Login main window](https://raw.githubusercontent.com/QianyeSu/GDOU-net-login/main/assets/gdou-net-login-main.png)

## Features

- 一键登录、断开和状态检测。
- 自动重连：离线后立即尝试重连，失败后按“重试间隔”继续尝试。
- 在线巡检：在线时只做低频状态检查，避免频繁认证。
- 自动识别 Portal、`ac_id` 和客户端 IP，也支持手动填写。
- 系统托盘后台运行，右键可显示主窗口、打开 GitHub、检查更新和退出。
- 密码保存到 Windows Credential Manager，不写入本地输入缓存。
- Skyborn 浅蓝、默认白色和暗色三套主题。
- 诊断视图用于排查 Portal、Challenge 和在线状态。

## Download

请从 [Releases](https://github.com/QianyeSu/GDOU-net-login/releases) 下载最新版本。

推荐下载 Windows 安装包；如果只想临时测试，也可以使用免安装 exe。

| 文件 | 用途 |
| --- | --- |
| `*-setup.exe` | 推荐安装包 |
| `*.msi` | MSI 安装包 |
| `gdou-net-login-windows-x86_64.exe` | 免安装运行 |
| `SHA256SUMS.txt` | 文件校验 |

当前 Windows 安装包暂未做代码签名，首次运行时可能出现 SmartScreen 提示。请只从本项目 Release 页面下载。

## Usage

1. 打开客户端。
2. 输入本人校园网账号和密码。
3. 保持“自动重连”开启。
4. 点击“登录”。
5. 最小化或关闭窗口后，程序会留在系统托盘继续运行。

如果首次安装后无法自动识别认证地址，可以打开“高级设置”，点击“自动探测 Portal”或“诊断”查看原因。

## How It Works

程序通过 SRUN Portal 认证接口完成登录。它会自动探测校园网认证地址、`ac_id` 和本机 IP，然后请求 challenge/token，按 SRUN 协议计算登录参数并提交认证请求。

后台自动重连逻辑会先检测当前是否在线。在线时只按“在线巡检”间隔做轻量状态检查；离线时立即尝试登录，失败后按“重试间隔”继续重试。

## Privacy

- 密码保存到系统凭据管理器，不保存到前端输入缓存。
- 本地输入缓存只用于恢复界面配置，例如主题、账号、Portal 等。
- 程序不会上传账号、密码或配置到第三方服务器。
- 自动重连只执行状态检测和正常登录请求，不修改校园网认证规则。

## Compliance

本软件不提供破解、绕过认证、共享账号、代理加速、访问控制规避等能力。使用者应遵守国家法律法规、学校网络安全管理规定和校园网使用规范，并对自己的网络行为负责。

## Development

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

构建免安装 exe：

```bash
cd desktop
../frontend/node_modules/.bin/tauri build --no-bundle
```

构建安装包：

```bash
cd desktop
../frontend/node_modules/.bin/tauri build
```

## License

GPL-3.0-only
