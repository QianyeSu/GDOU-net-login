# GDOU Net Login

广东海洋大学校园网自动登录与断线重连桌面客户端，适合需要长期保持电脑在线、远程连接实验室电脑，或避免网络断开后手动重新认证的日常场景。当前版本提供 Windows x64 与 macOS Apple Silicon 原生客户端。

> 本项目不是学校官方客户端，只用于本人已授权账号的正常校园网认证登录和连接保持。请遵守国家法律法规、学校网络安全管理规定和校园网使用规范；如学校或网络管理部门对第三方客户端、自动登录工具或认证方式有不同要求，应以官方规定为准。

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

| 文件 | 平台 | 用途 |
| --- | --- | --- |
| `gdou-net-login-macos-arm64.dmg` | macOS (15+ Apple Silicon) | 推荐，原生 Swift 客户端 DMG 磁盘映像 |
| `*-setup.exe` | Windows x64 | 推荐安装包 |
| `*.msi` | Windows x64 | MSI 安装包 |
| `gdou-net-login-windows-x86_64.exe` | Windows x64 | 免安装直接运行 |
| `SHA256SUMS.txt` | 全平台 | 文件哈希校验 |

首次运行提示说明：
- **Windows**：暂未做微软代码签名，首次运行时可能出现 SmartScreen 提示，点击“更多信息 -> 仍要运行”。
- **macOS**：原生 Swift 编写（针对 M 系列芯片）。首次打开若提示“无法验证开发者”，请前往「系统设置 -> 隐私与安全性」底部点击「仍要打开」，或在访达中右键点击 App 选择「打开」。请只从本项目 Release 页面下载。

## Usage

1. 打开客户端。
2. 输入本人校园网账号和密码。
3. 保持“自动重连”开启。
4. 点击“登录”。
5. 最小化或关闭窗口后，程序会留在系统托盘继续运行。

如果首次安装后无法自动识别认证地址，可以打开“高级设置”，点击“自动探测 Portal”或“诊断”查看原因。

## 网络路径说明

如果正在使用 Clash、Clash Verge Rev、V2RayN、sing-box、NekoRay 等网络工具，建议先区分两种情况：

- 只开启系统代理、规则模式或全局模式，但没有开启 TUN/虚拟网卡时，程序的 SRUN 登录请求仍会按后端直连逻辑访问校园网认证地址。
- 开启 TUN、虚拟网卡或透明代理时，系统路由可能会改变校园网 Portal 请求的出口，导致认证地址不可达或登录 IP 不正确。

程序不会关闭或修改第三方网络工具，也不会修改代理、DNS 或路由配置。诊断视图会尽量提示系统代理、TUN/虚拟网卡、Portal 路由和当前登录 IP。若提示 Portal 可能经过虚拟网卡/TUN，请在对应工具中把校园网网段设为直连，例如：

```yaml
- IP-CIDR,10.0.0.0/8,DIRECT
- IP-CIDR,172.16.0.0/12,DIRECT
- IP-CIDR,192.168.0.0/16,DIRECT
```

## How It Works

程序通过 SRUN Portal 认证接口完成登录。它会自动探测校园网认证地址、`ac_id` 和本机 IP，然后请求 challenge/token，按 SRUN 协议计算登录参数并提交认证请求。

后台自动重连逻辑会先检测当前是否在线。在线时只按“在线巡检”间隔做轻量状态检查；离线时立即尝试登录，失败后按“重试间隔”继续重试。

## Privacy

- 密码保存到系统凭据管理器，不保存到前端输入缓存。
- 本地输入缓存只用于恢复界面配置，例如主题、账号、Portal 等。
- 程序不会上传账号、密码或配置到第三方服务器。
- 自动重连只执行状态检测和正常登录请求，不修改校园网认证规则。
- 程序只允许访问 `http` / `https` 认证和探测地址，并拒绝明显危险的本机或链路本地地址。

## Compliance

本软件的设计边界是：使用本人已授权账号完成正常校园网认证，并在断线后按合理间隔重新发起正常登录请求。

本软件不提供，也不应被用于以下场景：

- 破解、绕过或规避校园网认证与访问控制。
- 使用、保存、传播或共享他人账号。
- 批量登录、批量探测、校园网扫描或高频重复请求认证接口。
- 代理加速、流量转发、账号共享或免认证访问。
- 修改、干扰、破坏学校网络服务、认证系统或其他用户的正常使用。

使用者应妥善保管本人账号密码，并对自己的网络行为负责。若学校后续调整认证规则、客户端使用政策或网络管理要求，请以学校和网络管理部门的规定为准。

## Development

前端位于 [frontend](file:///f:/PythonPackage/GDOU-net-login/frontend)，桌面端位于 [desktop](file:///f:/PythonPackage/GDOU-net-login/desktop)。

### 1. 开发调试（支持热重载 HMR）

方式一（推荐，一键启动前端与桌面端）：
```bash
cd frontend
npm install
npm run tauri dev
```

方式二（手动分终端启动）：
- 终端 1：`cd frontend && npm run dev`
- 终端 2：`cd desktop && cargo run`

### 2. 生产运行与本地构建

构建前端并运行 Release 版本（无需本地前端服务器）：
```bash
cd frontend
npm install
npm run build

cd ../desktop
cargo run --release
```

macOS Apple Silicon 原生客户端（需要 macOS 15+ 与 Swift 工具链）：
```bash
cd macos
bash scripts/build_app.sh -
```

脚本会在 `macos/build/GDOU-net-login.app` 生成应用，并在 `release-assets/gdou-net-login-macos-arm64.dmg` 生成 DMG。`-` 表示本地 ad-hoc 签名；分发给其他机器时应改用 Apple Developer 签名，并按实际网络环境验证 Portal、`ac_id` 与客户端 IP。

构建免安装 exe：
```bash
cd frontend
npm run tauri build -- --no-bundle
```

构建完整安装包（MSI / Setup）：
```bash
cd frontend
npm run tauri build
```

## License

BSD-3-Clause
