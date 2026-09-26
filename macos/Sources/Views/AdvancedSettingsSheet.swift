import SwiftUI
import AppKit

/// The macOS settings window intentionally follows the Windows client's
/// three-tab "设置与工具中心" layout.  The controls are native SwiftUI, but
/// the grouping and wording stay the same so moving between the two clients
/// does not require learning a second settings model.
public struct AdvancedSettingsSheet: View {
    @ObservedObject var manager: AutoReconnectManager
    @ObservedObject var monitor: NetworkMonitor
    @Environment(\.dismiss) private var dismiss

    @AppStorage("gdou_auto_check_update") private var autoCheckUpdate = false

    @State private var portalUrl = ""
    @State private var probeUrl = ""
    @State private var acidText = ""
    @State private var userIpText = ""
    @State private var bindIpText = ""
    @State private var bindIpExplicit = false
    @State private var autoQueryAcid = true
    @State private var retryText = "15"
    @State private var onlineCheckText = "60"
    @State private var autoReconnect = true
    @State private var startupEnabled = false
    @State private var rememberPassword = true
    @State private var selectedTab: SettingsTab = .general
    @State private var showAllInterfaces = false
    @State private var isWorking = false
    @State private var statusText: String?
    @State private var traces: [PortalProbeTrace] = []

    private enum SettingsTab: String, CaseIterable, Hashable {
        case general, diagnostic, network

        var title: String {
            switch self {
            case .general: return "常规偏好"
            case .diagnostic: return "网络诊断"
            case .network: return "高级网卡"
            }
        }

        var icon: String {
            switch self {
            case .general: return "slider.horizontal.3"
            case .diagnostic: return "stethoscope"
            case .network: return "network"
            }
        }
    }

    public var body: some View {
        VStack(spacing: 0) {
            header
            tabBar
            Divider().opacity(0.55)

            ScrollView(.vertical, showsIndicators: false) {
                VStack(alignment: .leading, spacing: 12) {
                    if selectedTab == .general {
                        generalContent
                    } else if selectedTab == .diagnostic {
                        diagnosticContent
                    } else {
                        networkContent
                    }
                }
                .padding(.horizontal, 18)
                .padding(.vertical, 14)
            }

            Divider().opacity(0.55)
            HStack {
                if let statusText, !statusText.isEmpty {
                    Text(statusText)
                        .font(.system(size: 10))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                Spacer()
                Button {
                    saveAndClose()
                } label: {
                    Label("保存并关闭", systemImage: "checkmark")
                        .font(.system(size: 12, weight: .semibold))
                }
                .keyboardShortcut(.defaultAction)
                .buttonStyle(.borderedProminent)
                .controlSize(.regular)
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 12)
            .background(.bar)
        }
        // Keep the settings sheet compact like the Windows tool window.  The
        // content remains scrollable, so advanced sections do not need to
        // cover the waveform and the rest of the main window underneath.
        .frame(minWidth: 500, idealWidth: 560, maxWidth: 620,
               minHeight: 430, idealHeight: 500, maxHeight: 600)
        .background(.regularMaterial)
        .onAppear {
            loadForm()
            // Reuse the main window's monitor.  Settings only needs an
            // interface snapshot and must not create a second timer.
            monitor.refreshInterfaces()
        }
    }

    private var header: some View {
        HStack(spacing: 9) {
            Image(systemName: "slider.horizontal.3")
                .font(.system(size: 15, weight: .bold))
                .foregroundStyle(.tint)
            Text("设置与工具中心")
                .font(.system(size: 16, weight: .bold))
            Spacer()
            Button {
                dismiss()
            } label: {
                Image(systemName: "xmark")
                    .font(.system(size: 11, weight: .bold))
                    .frame(width: 24, height: 24)
            }
            .buttonStyle(.borderless)
            .keyboardShortcut(.cancelAction)
            .help("关闭（不保存本次修改）")
        }
        .padding(.horizontal, 18)
        .padding(.top, 15)
        .padding(.bottom, 11)
    }

    private var tabBar: some View {
        HStack(spacing: 0) {
            ForEach(SettingsTab.allCases, id: \.self) { tab in
                Button {
                    selectedTab = tab
                    if tab == .network {
                        monitor.refreshInterfaces()
                    }
                } label: {
                    Label(tab.title, systemImage: tab.icon)
                        .font(.system(size: 11, weight: selectedTab == tab ? .semibold : .regular))
                        .frame(maxWidth: .infinity)
                        .frame(minHeight: 38)
                        .background(selectedTab == tab ? Color.accentColor.opacity(0.13) : Color.clear)
                        .foregroundStyle(selectedTab == tab ? Color.accentColor : .secondary)
                        .clipShape(RoundedRectangle(cornerRadius: 7, style: .continuous))
                        // The complete tab cell is a hit target, not just the
                        // icon/text glyph.  This prevents the empty green
                        // area between tabs from looking clickable but doing
                        // nothing.
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .contentShape(Rectangle())
            }
        }
        .padding(.horizontal, 14)
        .padding(.bottom, 8)
    }

    private var generalContent: some View {
        VStack(alignment: .leading, spacing: 12) {
            settingToggle(
                title: "开机启动",
                description: "macOS 启动后自动在后台托盘守护",
                isOn: $startupEnabled
            )
            settingToggle(
                title: "自动重连守护",
                description: "网络断开后自动尝试重新认证",
                isOn: $autoReconnect
            )
            settingToggle(
                title: "自动检测更新",
                description: "联网成功后静默检测是否有新版本发布",
                isOn: $autoCheckUpdate
            )

            sectionCard {
                sectionTitle("定时巡检与重试间隔")
                HStack(spacing: 10) {
                    numberField(title: "失败重试 (秒)", text: $retryText, placeholder: "15")
                    numberField(title: "在线巡检 (秒)", text: $onlineCheckText, placeholder: "60")
                }
                Text("失败重试最短 5 秒，在线巡检最短 15 秒。")
                    .font(.system(size: 10))
                    .foregroundStyle(.secondary)
            }

            sectionCard {
                HStack(spacing: 8) {
                    Image(systemName: "clock.arrow.circlepath")
                        .foregroundStyle(.tint)
                    VStack(alignment: .leading, spacing: 3) {
                        HStack(spacing: 6) {
                            Text("最近一次重连")
                                .font(.system(size: 12, weight: .semibold))
                            Text(reconnectDateText)
                                .font(.system(size: 11, weight: .semibold, design: .monospaced))
                                .foregroundStyle(.tint)
                        }
                        Text(manager.lastReconnectAt == nil
                             ? "暂未记录成功的自动重连或重连自测"
                             : "最近一次成功自动重连或重连自测的时间")
                            .font(.system(size: 10))
                            .foregroundStyle(.secondary)
                    }
                    Spacer(minLength: 4)
                }
                HStack(spacing: 8) {
                    Button {
                        checkForUpdates()
                    } label: {
                        Label("检查更新", systemImage: "arrow.clockwise")
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                }
            }

            // Passwords are never serialized into config.json on macOS.  Keep
            // this switch visible here so the behavior is explicit instead of
            // surprising users who came from the Windows client.
            settingToggle(
                title: "使用钥匙串保存密码",
                description: "密码只保存在此 Mac 的登录钥匙串，不会写入 config.json",
                isOn: $rememberPassword
            )

            HStack(spacing: 7) {
                Image(systemName: keychainStatusIcon)
                    .foregroundStyle(keychainStatusColor)
                Text(keychainStatusText)
                    .font(.system(size: 10))
                    .foregroundStyle(.secondary)
            }
            .padding(.horizontal, 4)
        }
    }

    private var keychainStatusText: String {
        switch KeychainService.passwordStatus(forAccount: manager.config.username) {
        case .available:
            return "已找到当前账号的钥匙串密码，可用于自动重连"
        case .missing:
            return "尚未保存当前账号密码；成功登录并开启保存后会写入钥匙串"
        case .inaccessible:
            return "钥匙串暂时无法访问；首次提示时请选择“允许”"
        }
    }

    private var keychainStatusIcon: String {
        switch KeychainService.passwordStatus(forAccount: manager.config.username) {
        case .available: return "checkmark.shield.fill"
        case .missing: return "info.circle"
        case .inaccessible: return "exclamationmark.triangle.fill"
        }
    }

    private var keychainStatusColor: Color {
        switch KeychainService.passwordStatus(forAccount: manager.config.username) {
        case .available: return .green
        case .missing: return .secondary
        case .inaccessible: return .orange
        }
    }

    private var diagnosticContent: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 6) {
                diagnosticButton("在线检测", icon: "checkmark.circle") {
                    runOnlineCheck()
                }
                diagnosticButton("重连自测", icon: "arrow.triangle.2.circlepath") {
                    runReconnectSelfTest()
                }
                diagnosticButton("重新诊断", icon: "stethoscope") {
                    runDiagnosis()
                }
            }

            if let statusText, !statusText.isEmpty {
                statusCard(title: diagnosisTitle, detail: statusText, isOnline: diagnosisOnlineState)
            } else {
                statusCard(title: "诊断就绪", detail: "点击上方按钮即可发起网络与认证探测", isOnline: nil)
            }

            let detectedPortal = traces.compactMap(\.portalUrl).first
                ?? (portalUrl.isEmpty ? nil : portalUrl)
            let detectedAcid = traces.compactMap(\.acid).first
                ?? UInt32(acidText)
            let currentIPs = monitor.interfaces.map(\.ip)
            let detectedIP = [userIpText.nonEmptyOrNil, traces.compactMap(\.userIp).first]
                .compactMap { $0 }
                .first(where: { currentIPs.contains($0) })
                ?? monitor.interfaces.first?.ip
                ?? userIpText.nonEmptyOrNil

            LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 7) {
                diagnosticValue("Portal 地址", detectedPortal ?? "-")
                diagnosticValue("ac_id", detectedAcid.map(String.init) ?? "-")
                diagnosticValue("登录出口网卡", bindIpText.nonEmptyOrNil ?? "自动选择")
                diagnosticValue("客户端 IP", detectedIP ?? "-")
            }

            if !traces.isEmpty {
                VStack(alignment: .leading, spacing: 6) {
                    Text("探测明细")
                        .font(.system(size: 11, weight: .semibold))
                    ForEach(traces) { trace in
                        HStack(alignment: .top, spacing: 7) {
                            Circle()
                                .fill(trace.status == 200 || trace.status == 302 ? Color.green : Color.orange)
                                .frame(width: 6, height: 6)
                                .padding(.top, 4)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(trace.target)
                                    .font(.system(size: 10, design: .monospaced))
                                    .lineLimit(1)
                                Text(traceSummary(trace))
                                    .font(.system(size: 9))
                                    .foregroundStyle(.secondary)
                                    .lineLimit(2)
                            }
                            Spacer(minLength: 0)
                        }
                    }
                }
                .padding(10)
                .background(Color.primary.opacity(0.035))
                .clipShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
            }
        }
    }

    private var networkContent: some View {
        VStack(alignment: .leading, spacing: 12) {
            sectionCard {
                HStack {
                    sectionTitle("登录出口网卡")
                    Spacer()
                    Button {
                        monitor.refreshInterfaces()
                    } label: {
                        Label("刷新", systemImage: "arrow.clockwise")
                            .font(.system(size: 10, weight: .semibold))
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                }

                Picker("登录出口网卡", selection: selectedInterfaceBinding) {
                    Text("自动选择（推荐）").tag("")
                    ForEach(visibleInterfaces) { iface in
                        Text("\(iface.displayName) / \(iface.ip)")
                            .tag(iface.ip)
                    }
                }
                .labelsHidden()
                .pickerStyle(.menu)
                .frame(maxWidth: .infinity, alignment: .leading)

                Toggle("显示全部接口", isOn: $showAllInterfaces)
                    .toggleStyle(.checkbox)
                    .font(.system(size: 11))
                Text("自动选择会优先使用当前 Wi-Fi/以太网 IPv4；保存的旧 DHCP 地址不会被强制提交。")
                    .font(.system(size: 10))
                    .foregroundStyle(.secondary)
            }

            sectionCard {
                HStack {
                    sectionTitle("认证与探测参数")
                    Spacer()
                    Button {
                        autoDetectPortal()
                    } label: {
                        if isWorking {
                            ProgressView().controlSize(.small)
                        } else {
                            Label("自动探测", systemImage: "magnifyingglass")
                                .font(.system(size: 10, weight: .semibold))
                        }
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .disabled(isWorking)
                }

                HStack(spacing: 10) {
                    textField(title: "Portal 地址", text: $portalUrl, placeholder: "http://10.129.1.1")
                    textField(title: "ac_id", text: $acidText, placeholder: "17")
                }
                HStack(spacing: 10) {
                    textField(title: "探测连通性地址", text: $probeUrl, placeholder: "http://www.msftconnecttest.com/connecttest.txt")
                    textField(title: "客户端 IP", text: $userIpText, placeholder: "自动获取")
                }
                Toggle("自动识别接入点 (ac_id)", isOn: $autoQueryAcid)
                    .toggleStyle(.checkbox)
                    .font(.system(size: 11))
            }

            if !monitor.interfaces.isEmpty {
                sectionCard {
                    sectionTitle("当前 IPv4 接口")
                    ForEach(visibleInterfaces) { iface in
                        HStack(spacing: 8) {
                            Image(systemName: iface.isWifi ? "wifi" : "network")
                                .foregroundStyle(iface.isUp ? .green : .secondary)
                            Text(iface.displayName)
                                .font(.system(size: 11, weight: .medium))
                            Spacer()
                            Text(iface.ip)
                                .font(.system(size: 10, design: .monospaced))
                                .foregroundStyle(.secondary)
                            Button("使用") {
                                bindIpText = iface.ip
                                bindIpExplicit = true
                                userIpText = iface.ip
                            }
                            .buttonStyle(.bordered)
                            .controlSize(.mini)
                        }
                    }
                }
            }
        }
    }

    private var visibleInterfaces: [NetworkInterfaceItem] {
        if showAllInterfaces { return monitor.interfaces }
        return monitor.interfaces.filter { iface in
            !iface.ip.hasPrefix("127.") && !iface.ip.hasPrefix("169.254.") && !iface.name.lowercased().contains("bluetooth")
        }
    }

    private var selectedInterfaceBinding: Binding<String> {
        Binding(
            get: { bindIpText },
            set: { value in
                bindIpText = value
                bindIpExplicit = !value.isEmpty
                if !value.isEmpty { userIpText = value }
            }
        )
    }

    private var diagnosisTitle: String {
        if isWorking { return "诊断中" }
        if diagnosisFailed { return "重连自测失败" }
        if statusText?.contains("握手") == true { return "Portal 诊断" }
        return "诊断完成"
    }

    private var diagnosisFailed: Bool {
        guard let statusText else { return false }
        let lower = statusText.lowercased()
        return statusText.contains("未成功")
            || statusText.contains("失败")
            || statusText.contains("错误")
            || statusText.contains("无法开始")
            || lower.contains("sign_error")
            || lower.contains("login_error")
    }

    private var diagnosisOnlineState: Bool? {
        guard !isWorking else { return nil }
        if diagnosisFailed { return false }
        guard let statusText else { return nil }
        return statusText.contains("正常")
            || statusText.contains("已恢复在线")
            || manager.isOnline
    }

    private var reconnectDateText: String {
        guard let date = manager.lastReconnectAt else { return "-" }
        return Self.dateFormatter.string(from: date)
    }

    private static let dateFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "zh_CN")
        formatter.dateFormat = "yyyy-MM-dd HH:mm:ss"
        return formatter
    }()

    @ViewBuilder
    private func settingToggle(title: String, description: String, isOn: Binding<Bool>) -> some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 3) {
                Text(title)
                    .font(.system(size: 12, weight: .semibold))
                Text(description)
                    .font(.system(size: 10))
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 8)
            Toggle("", isOn: isOn)
                .labelsHidden()
                .toggleStyle(.switch)
                .controlSize(.small)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .background(Color.primary.opacity(0.035))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(Color.primary.opacity(0.08), lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
    }

    @ViewBuilder
    private func sectionCard<Content: View>(@ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 9, content: content)
            .padding(11)
            .background(Color.primary.opacity(0.035))
            .overlay(RoundedRectangle(cornerRadius: 10).stroke(Color.primary.opacity(0.08), lineWidth: 1))
            .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
    }

    private func sectionTitle(_ title: String) -> some View {
        Text(title)
            .font(.system(size: 11, weight: .bold))
            .foregroundStyle(.secondary)
    }

    @ViewBuilder
    private func textField(title: String, text: Binding<String>, placeholder: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title).font(.system(size: 10, weight: .medium)).foregroundStyle(.secondary)
            TextField(placeholder, text: text)
                .textFieldStyle(.roundedBorder)
                .font(.system(size: 11))
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder
    private func numberField(title: String, text: Binding<String>, placeholder: String) -> some View {
        textField(title: title, text: text, placeholder: placeholder)
    }

    private func diagnosticButton(_ title: String, icon: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Label(title, systemImage: icon)
                .font(.system(size: 10, weight: .semibold))
                .frame(maxWidth: .infinity)
                .padding(.vertical, 8)
                .foregroundStyle(.primary)
                .background(Color.primary.opacity(0.045))
                .overlay(
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .stroke(Color.primary.opacity(0.08), lineWidth: 1)
                )
                .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        }
        .buttonStyle(.plain)
        .disabled(isWorking)
        .opacity(isWorking ? 0.48 : 1)
    }

    private func statusCard(title: String, detail: String, isOnline: Bool?) -> some View {
        HStack(spacing: 9) {
            Circle()
                .fill(isOnline == true ? Color.green : (isOnline == false ? Color.red : Color.orange))
                .frame(width: 9, height: 9)
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.system(size: 12, weight: .semibold))
                Text(detail).font(.system(size: 10)).foregroundStyle(.secondary).lineLimit(2)
            }
            Spacer()
        }
        .padding(10)
        .background(Color.primary.opacity(0.035))
        .overlay(RoundedRectangle(cornerRadius: 9).stroke(Color.primary.opacity(0.08), lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
    }

    private func diagnosticValue(_ title: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(title).font(.system(size: 9)).foregroundStyle(.secondary)
            Text(value)
                .font(.system(size: 10, design: .monospaced))
                .lineLimit(2)
                .textSelection(.enabled)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(8)
        .background(Color.primary.opacity(0.035))
        .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
    }

    private func traceSummary(_ trace: PortalProbeTrace) -> String {
        if let status = trace.status {
            let target = trace.portalUrl.map { " → \($0)" } ?? ""
            return "HTTP \(status)\(target)"
        }
        return trace.error ?? "未收到响应"
    }

    private func loadForm() {
        portalUrl = manager.config.portalUrl
        probeUrl = manager.config.probeUrl
        autoQueryAcid = manager.config.autoQueryAcid
        acidText = manager.config.acid.map(String.init) ?? ""
        userIpText = manager.config.userIp ?? ""
        bindIpExplicit = manager.config.bindIpExplicit == true
        // Legacy configs did not record whether a bind was deliberate.  Show
        // them as automatic instead of presenting a stale DHCP lease as the
        // current outlet; the resolver also treats these values as fallback.
        bindIpText = bindIpExplicit ? (manager.config.bindIp ?? "") : ""
        retryText = String(manager.config.retrySeconds)
        onlineCheckText = String(manager.config.onlineCheckSeconds)
        autoReconnect = manager.config.autoReconnect
        startupEnabled = manager.config.startupEnabled
        rememberPassword = manager.config.rememberPassword
    }

    private func saveAndClose() {
        manager.config.portalUrl = portalUrl.trimmed
        manager.config.probeUrl = probeUrl.trimmed
        manager.config.autoQueryAcid = autoQueryAcid
        manager.config.userIp = userIpText.nonEmptyOrNil
        manager.config.bindIp = bindIpText.nonEmptyOrNil
        manager.config.bindIpExplicit = bindIpText.nonEmptyOrNil == nil ? false : bindIpExplicit
        if !autoQueryAcid, let acid = UInt32(acidText.trimmed), acid > 0 {
            manager.config.acid = acid
        } else if autoQueryAcid {
            manager.config.acid = nil
        }
        manager.config.retrySeconds = max(5, min(3600, UInt64(retryText.trimmed) ?? 15))
        manager.config.onlineCheckSeconds = max(15, min(3600, UInt64(onlineCheckText.trimmed) ?? 60))
        manager.config.autoReconnect = autoReconnect
        manager.config.startupEnabled = startupEnabled
        manager.config.rememberPassword = rememberPassword
        if #available(macOS 13.0, *) {
            _ = LaunchAtLogin.setEnabled(startupEnabled)
        }
        manager.updateConfig(manager.config)
        dismiss()
    }

    private func autoDetectPortal() {
        isWorking = true
        statusText = "正在探测 Portal、ac_id 和当前客户端 IP..."
        Task {
            let client = SrunClient(config: manager.config)
            do {
                let detected = try await client.probePortalFast()
                await MainActor.run {
                    if let portal = detected.portalUrl { portalUrl = portal }
                    if let acid = detected.acid { acidText = String(acid) }
                    if let ip = detected.userIp, monitor.interfaces.contains(where: { $0.ip == ip }) {
                        userIpText = ip
                        bindIpText = ""
                        bindIpExplicit = false
                    } else if let ip = monitor.interfaces.first?.ip {
                        // Keep automatic outlet selection by default; only
                        // update the client IP field to the current lease.
                        userIpText = ip
                    }
                    statusText = "自动探测完成"
                    isWorking = false
                }
            } catch {
                await MainActor.run {
                    statusText = "自动探测失败：\(error.localizedDescription)"
                    isWorking = false
                }
            }
        }
    }

    private func runDiagnosis() {
        guard !isWorking else { return }
        isWorking = true
        statusText = "正在探测 Portal、Challenge 和当前网卡..."
        Task {
            let client = SrunClient(config: manager.config)
            let newTraces = await client.probeDetailedTraces()
            var message = "探测完成"
            do {
                let detected = try await client.probePortalFast()
                let portal = detected.portalUrl ?? portalUrl.nonEmptyOrNil ?? "http://172.16.200.11"
                let acid = detected.acid ?? UInt32(acidText) ?? manager.config.acid ?? 1
                let currentIPs = monitor.interfaces.map(\.ip)
                let configuredCandidates = [bindIpText.nonEmptyOrNil, userIpText.nonEmptyOrNil, detected.userIp]
                    .compactMap { $0 }
                // A portal redirect can echo a DHCP address from an older
                // session.  Prefer a value that is present on this Mac now.
                // This keeps diagnostics consistent with the login resolver.
                let ip = configuredCandidates.first(where: { currentIPs.contains($0) })
                    ?? monitor.interfaces.first?.ip
                    ?? (currentIPs.isEmpty ? configuredCandidates.first : nil)
                if let ip {
                    let token = try await client.getChallenge(portalUrl: portal, ip: ip, acid: acid)
                    message = "Challenge 握手正常（Portal=\(portal), ac_id=\(acid), IP=\(ip), Token=\(token.prefix(8))…）"
                } else {
                    message = "探测完成，但未找到可用客户端 IPv4"
                }
            } catch {
                message = "探测完成；Challenge 握手失败：\(error.localizedDescription)"
            }
            await MainActor.run {
                traces = newTraces
                statusText = message
                isWorking = false
            }
        }
    }

    private func runOnlineCheck() {
        guard !isWorking else { return }
        isWorking = true
        statusText = "正在检查 Portal 在线状态..."
        Task {
            await manager.checkStatus()
            await MainActor.run {
                statusText = manager.statusDetail ?? manager.statusText
                isWorking = false
            }
        }
    }

    private func runReconnectSelfTest() {
        guard !isWorking else { return }
        isWorking = true
        statusText = "正在执行重连自测..."
        Task {
            await manager.reconnectSelfTest()
            // Refresh the diagnostic context after the real reconnect.  The
            // authentication error itself contains the resolved Portal/IP,
            // but leaving these cards as "-" makes a failed self-test look
            // like the macOS client never probed anything.
            let detected = try? await SrunClient(config: manager.config).probePortalFast()
            await MainActor.run {
                if let portal = manager.lastLoginPortal ?? detected?.portalUrl { portalUrl = portal }
                if let acid = manager.lastLoginAcid ?? detected?.acid { acidText = String(acid) }
                if let ip = manager.lastLoginIP ?? detected?.userIp ?? manager.config.userIp {
                    userIpText = ip
                }
                statusText = manager.errorMessage ?? manager.statusDetail ?? manager.statusText
                isWorking = false
            }
        }
    }

    private func checkForUpdates() {
        // Do not send the user to the repository from a settings click.  A
        // signed macOS updater has not been wired into this target yet.
        statusText = "macOS 自动更新暂未配置"
    }
}

private extension String {
    var trimmed: String { trimmingCharacters(in: .whitespacesAndNewlines) }
    var nonEmptyOrNil: String? {
        let value = trimmed
        return value.isEmpty ? nil : value
    }
}
