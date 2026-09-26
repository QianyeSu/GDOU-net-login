import SwiftUI
import AppKit

public struct ContentView: View {
    @StateObject private var manager = AutoReconnectManager()
    @StateObject private var monitor = NetworkMonitor()

    @State private var password: String = ""
    @State private var showPassword: Bool = false
    @State private var showingSettings = false
    @State private var showingDiagnostics = false
    @State private var isRefreshing = false

    @AppStorage("gdou_theme_mode") private var themeMode: String = "light"
    @Environment(\.colorScheme) private var systemColorScheme
    @Environment(\.scenePhase) private var scenePhase

    public init() {}

    private var activeColorScheme: ColorScheme {
        if themeMode == "dark" {
            return .dark
        } else {
            return .light
        }
    }

    public var body: some View {
        ZStack {
            // Main Window Background (Clean, crisp, high-end card)
            mainBackground
                .ignoresSafeArea()

            VStack(spacing: 0) {
                // Top Traffic Lights Bar
                HStack {
                    // Traffic lights are on the top-left natively
                    Spacer()
                }
                .frame(height: 24)
                .padding(.top, 4)

                // Main Scrollable / Resizable Content
                ScrollView(showsIndicators: false) {
                    VStack(spacing: 9) {
                    // 1. School Badge & Header
                    VStack(spacing: 4) {
                        SchoolBadgeView()
                            .padding(.bottom, 2)

                        Text("广东海洋大学")
                            .font(.system(size: 16, weight: .bold))
                            .foregroundColor(primaryTextColor)
                            .tracking(0.5)

                        HStack(spacing: 5) {
                            Circle()
                                .fill(Color.blue)
                                .frame(width: 5, height: 5)
                            Text("校园网助手")
                                .font(.system(size: 11, weight: .semibold))
                                .foregroundColor(.blue)
                        }
                        .padding(.horizontal, 9)
                        .padding(.vertical, 2.5)
                        .background(Color.blue.opacity(0.08))
                        .cornerRadius(999)
                        .overlay(
                            Capsule().stroke(Color.blue.opacity(0.22), lineWidth: 1)
                        )
                    }
                    .padding(.bottom, 2)
                    .windowDragArea()

                    // 2. Network Status Capsule.  Showing the last Portal
                    // detail here prevents a failed macOS request from looking
                    // like an endless spinner with no explanation.
                    VStack(spacing: 3) {
                        statusCapsuleView
                        if let detail = manager.errorMessage ?? manager.statusDetail,
                           !detail.isEmpty,
                           !manager.isOnline,
                           manager.statusText != "离线未连接" {
                            Text(detail)
                                .font(.system(size: 9.5))
                                .foregroundColor(.secondary)
                                .lineLimit(2)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .padding(.horizontal, 6)
                        }
                    }
                    .windowDragArea()

                    // 3. Login Inputs & Primary Action
                    VStack(spacing: 8) {
                        // Username Field
                        VStack(alignment: .leading, spacing: 4) {
                            Text("校园网账号")
                                .font(.system(size: 11.5, weight: .bold))
                                .foregroundColor(secondaryTextColor)

                            TextField("请输入学号 / 工号", text: $manager.config.username)
                                .textFieldStyle(.plain)
                                .font(.system(size: 13))
                                .padding(.horizontal, 10)
                                .frame(height: 35)
                                .background(inputBackgroundColor)
                                .cornerRadius(8)
                                .overlay(
                                    RoundedRectangle(cornerRadius: 8)
                                        .stroke(borderColor, lineWidth: 1)
                                )
                                .onChange(of: manager.config.username) { _, _ in
                                    manager.updateConfig(manager.config)
                                    loadSavedPassword()
                                }
                        }

                        // Password Field
                        VStack(alignment: .leading, spacing: 4) {
                            Text("认证密码")
                                .font(.system(size: 11.5, weight: .bold))
                                .foregroundColor(secondaryTextColor)

                            HStack {
                                if showPassword {
                                    TextField("请输入密码", text: $password)
                                        .textFieldStyle(.plain)
                                        .font(.system(size: 13))
                                } else {
                                    SecureField("请输入密码", text: $password)
                                        .textFieldStyle(.plain)
                                        .font(.system(size: 13))
                                }

                                Button(action: { showPassword.toggle() }) {
                                    Image(systemName: showPassword ? "eye.slash.fill" : "eye.fill")
                                        .font(.system(size: 13))
                                        .foregroundColor(Color.secondary.opacity(0.7))
                                }
                                .buttonStyle(.plain)
                            }
                            .padding(.horizontal, 10)
                            .frame(height: 35)
                            .background(inputBackgroundColor)
                            .cornerRadius(8)
                            .overlay(
                                RoundedRectangle(cornerRadius: 8)
                                    .stroke(borderColor, lineWidth: 1)
                            )
                        }

                        // Auto Reconnect Checkbox
                        HStack(spacing: 7) {
                            Button(action: {
                                manager.config.autoReconnect.toggle()
                                manager.updateConfig(manager.config)
                            }) {
                                Image(systemName: manager.config.autoReconnect ? "checkmark.square.fill" : "square")
                                    .font(.system(size: 15))
                                    .foregroundColor(manager.config.autoReconnect ? Color(red: 0.13, green: 0.77, blue: 0.37) : Color.secondary)
                            }
                            .buttonStyle(.plain)

                            Text("保持后台自动守护与断线重连")
                                .font(.system(size: 11.5, weight: .medium))
                                .foregroundColor(primaryTextColor)

                            Spacer()
                        }
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                        .background(subtleBackgroundColor)
                        .cornerRadius(7)

                        // Big Action Button (Vibrant Emerald Green)
                        Button(action: handlePrimaryAction) {
                            HStack(spacing: 8) {
                                if manager.isConnecting {
                                    ProgressView()
                                        .scaleEffect(0.75)
                                } else if manager.isOnline {
                                    Image(systemName: "power")
                                        .font(.system(size: 14, weight: .bold))
                                } else {
                                    Image(systemName: "arrow.right.circle.fill")
                                        .font(.system(size: 14, weight: .bold))
                                }

                                Text(buttonTitle)
                                    .font(.system(size: 14, weight: .bold))
                            }
                            .foregroundColor(.white)
                            .frame(maxWidth: .infinity)
                            .frame(height: 38)
                            .background(buttonGradient)
                            .cornerRadius(9)
                            .shadow(color: buttonShadowColor, radius: 8, x: 0, y: 3)
                        }
                        .buttonStyle(.plain)
                        .disabled(manager.isConnecting)
                    }

                    // 4. Real-time Traffic Waveform Card
                    TrafficWaveformView(monitor: monitor)

                    // 5. Session Statistics (Duration & Cumulative Traffic)
                    HStack(spacing: 8) {
                        VStack(spacing: 2) {
                            Text(monitor.formattedDuration)
                                .font(.system(size: 13, weight: .bold, design: .monospaced))
                                .foregroundColor(primaryTextColor)
                            Text("本次在线时长")
                                .font(.system(size: 9.5, weight: .semibold))
                                .foregroundColor(secondaryTextColor)
                        }
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 6)
                        .background(subtleBackgroundColor)
                        .cornerRadius(8)
                        .overlay(RoundedRectangle(cornerRadius: 8).stroke(borderColor, lineWidth: 1))

                        VStack(spacing: 2) {
                            Text(monitor.formattedTotalTraffic)
                                .font(.system(size: 13, weight: .bold, design: .monospaced))
                                .foregroundColor(primaryTextColor)
                            Text("网卡累计流量")
                                .font(.system(size: 9.5, weight: .semibold))
                                .foregroundColor(secondaryTextColor)
                        }
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 6)
                        .background(subtleBackgroundColor)
                        .cornerRadius(8)
                        .overlay(RoundedRectangle(cornerRadius: 8).stroke(borderColor, lineWidth: 1))
                    }
                    .windowDragArea()

                    // 6. Bottom Toolbar
                    HStack {
                        // Left: Theme toggle + Refresh
                        HStack(spacing: 4) {
                            HStack(spacing: 2) {
                                Button(action: { themeMode = "light" }) {
                                    Image(systemName: "sun.max")
                                        .font(.system(size: 11, weight: themeMode == "light" ? .bold : .regular))
                                        .foregroundColor(themeMode == "light" ? .blue : .secondary)
                                        .frame(width: 22, height: 22)
                                        .background(themeMode == "light" ? Color.primary.opacity(0.08) : Color.clear)
                                        .cornerRadius(4)
                                }
                                .buttonStyle(.plain)

                                Button(action: { themeMode = "dark" }) {
                                    Image(systemName: "moon")
                                        .font(.system(size: 11, weight: themeMode == "dark" ? .bold : .regular))
                                        .foregroundColor(themeMode == "dark" ? .blue : .secondary)
                                        .frame(width: 22, height: 22)
                                        .background(themeMode == "dark" ? Color.primary.opacity(0.08) : Color.clear)
                                        .cornerRadius(4)
                                }
                                .buttonStyle(.plain)
                            }
                            .padding(2)
                            .background(subtleBackgroundColor)
                            .cornerRadius(6)
                            .overlay(RoundedRectangle(cornerRadius: 6).stroke(borderColor, lineWidth: 1))

                            Button(action: refreshStatus) {
                                Image(systemName: "arrow.clockwise")
                                    .font(.system(size: 11, weight: .medium))
                                    .foregroundColor(.secondary)
                                    .frame(width: 26, height: 26)
                                    .background(subtleBackgroundColor)
                                    .cornerRadius(6)
                                    .overlay(RoundedRectangle(cornerRadius: 6).stroke(borderColor, lineWidth: 1))
                                    .rotationEffect(.degrees(isRefreshing ? 360 : 0))
                                    .animation(isRefreshing ? Animation.linear(duration: 0.8).repeatForever(autoreverses: false) : .default, value: isRefreshing)
                            }
                            .buttonStyle(.plain)
                            .help("刷新校园网在线状态")
                        }

                        Spacer()

                        // Right: diagnostics and settings.  The Windows
                        // client does not show an always-visible repository
                        // link or build number in this footer.
                        HStack(spacing: 8) {
                            Button(action: { showingDiagnostics = true }) {
                                Image(systemName: "stethoscope")
                                    .font(.system(size: 11))
                                    .foregroundColor(.secondary)
                                    .frame(width: 26, height: 26)
                                    .background(subtleBackgroundColor)
                                    .cornerRadius(6)
                                    .overlay(RoundedRectangle(cornerRadius: 6).stroke(borderColor, lineWidth: 1))
                            }
                            .buttonStyle(.plain)
                            .help("诊断 Portal、Challenge 和网卡路径")

                            Button(action: { showingSettings = true }) {
                                HStack(spacing: 4) {
                                    Image(systemName: "gearshape")
                                        .font(.system(size: 11))
                                    Text("设置")
                                        .font(.system(size: 11, weight: .medium))
                                }
                                .foregroundColor(primaryTextColor)
                                .padding(.horizontal, 8)
                                .padding(.vertical, 4)
                                .background(subtleBackgroundColor)
                                .cornerRadius(6)
                                .overlay(RoundedRectangle(cornerRadius: 6).stroke(borderColor, lineWidth: 1))
                            }
                            .buttonStyle(.plain)

                        }
                    }
                    .padding(.top, 4)
                    .windowDragArea()
                }
                }
                .padding(.horizontal, 16)
                .padding(.bottom, 16)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        // Match the Rust/Tauri layout: 388x655 is the compact default, while
        // the card and waveform expand with a user-resized window.  The
        // window's full-screen control is still hidden in AppDelegate, so
        // resizing does not reintroduce the black full-screen bug.
        .frame(minWidth: 388, idealWidth: 388, maxWidth: .infinity,
               minHeight: 655, idealHeight: 655, maxHeight: .infinity)
        .preferredColorScheme(activeColorScheme)
        .sheet(isPresented: $showingSettings) {
            AdvancedSettingsSheet(manager: manager, monitor: monitor)
        }
        .sheet(isPresented: $showingDiagnostics) {
            DiagnosticsSheet(manager: manager, monitor: monitor)
        }
        .onAppear {
            loadSavedPassword()
            monitor.startMonitoring()
            monitor.isOnline = manager.isOnline
        }
        .onChange(of: manager.isOnline) { _, newValue in
            monitor.isOnline = newValue
        }
        .onChange(of: scenePhase) { _, phase in
            // Keep authentication/reconnect alive in the manager, but stop
            // the one-second interface sampler while the window is hidden or
            // the app is backgrounded.  This mirrors the Rust frontend's
            // hidden-tab throttling and avoids needless background work.
            if phase == .active {
                monitor.startMonitoring()
            } else {
                monitor.stopMonitoring()
            }
        }
        .onDisappear {
            monitor.stopMonitoring()
        }
    }

    // MARK: - Subviews & Styles

    private var statusCapsuleView: some View {
        HStack(spacing: 8) {
            Circle()
                .fill(statusDotColor)
                .frame(width: 8, height: 8)
                .shadow(color: statusDotColor.opacity(0.8), radius: 4)

            Text(manager.statusText)
                .font(.system(size: 12, weight: .bold))
                .foregroundColor(primaryTextColor)

            if let ip = manager.lastLoginIP
                ?? monitor.interfaces.first(where: { $0.isWifi || $0.name.starts(with: "en") })?.ip {
                Text("IP: \(ip)")
                    .font(.system(size: 11, weight: .medium, design: .monospaced))
                    .foregroundColor(.secondary)
            }

            Spacer()
        }
        .padding(.horizontal, 14)
        .frame(height: 32)
        .background(statusCapsuleBackground)
        .cornerRadius(999)
        .overlay(
            Capsule().stroke(statusCapsuleBorder, lineWidth: 1)
        )
    }

    private var statusDotColor: Color {
        if manager.isConnecting {
            return Color(red: 0.96, green: 0.62, blue: 0.04) // Amber
        } else if manager.isOnline {
            return Color(red: 0.13, green: 0.77, blue: 0.37) // Green
        } else {
            return Color(red: 0.94, green: 0.27, blue: 0.27) // Red
        }
    }

    private var statusCapsuleBackground: Color {
        if manager.isConnecting {
            return Color(red: 0.98, green: 0.64, blue: 0.08).opacity(0.09)
        } else if manager.isOnline {
            return Color(red: 0.13, green: 0.77, blue: 0.37).opacity(0.09)
        } else {
            return Color(red: 0.94, green: 0.27, blue: 0.27).opacity(0.08)
        }
    }

    private var statusCapsuleBorder: Color {
        if manager.isConnecting {
            return Color(red: 0.98, green: 0.64, blue: 0.08).opacity(0.3)
        } else if manager.isOnline {
            return Color(red: 0.13, green: 0.77, blue: 0.37).opacity(0.3)
        } else {
            return Color(red: 0.94, green: 0.27, blue: 0.27).opacity(0.25)
        }
    }

    private var buttonTitle: String {
        if manager.isConnecting {
            return "正在连接..."
        } else if manager.isOnline {
            return "断开连接"
        } else {
            return "立即连接"
        }
    }

    private var buttonGradient: LinearGradient {
        if manager.isOnline {
            return LinearGradient(
                colors: [Color(red: 0.94, green: 0.27, blue: 0.27), Color(red: 0.86, green: 0.15, blue: 0.15)],
                startPoint: .top,
                endPoint: .bottom
            )
        } else {
            return LinearGradient(
                colors: [Color(red: 0.14, green: 0.78, blue: 0.40), Color(red: 0.09, green: 0.65, blue: 0.30)],
                startPoint: .top,
                endPoint: .bottom
            )
        }
    }

    private var buttonShadowColor: Color {
        if manager.isOnline {
            return Color.red.opacity(0.28)
        } else {
            return Color(red: 0.13, green: 0.77, blue: 0.37).opacity(0.32)
        }
    }

    private var mainBackground: some View {
        Group {
            if activeColorScheme == .dark {
                Color(red: 0.06, green: 0.09, blue: 0.15)
            } else {
                Color(red: 0.98, green: 0.98, blue: 0.99)
            }
        }
    }

    private var primaryTextColor: Color {
        activeColorScheme == .dark ? Color.white : Color(red: 0.06, green: 0.09, blue: 0.16)
    }

    private var secondaryTextColor: Color {
        activeColorScheme == .dark ? Color(red: 0.70, green: 0.75, blue: 0.82) : Color(red: 0.32, green: 0.38, blue: 0.46)
    }

    private var subtleBackgroundColor: Color {
        activeColorScheme == .dark ? Color(red: 0.12, green: 0.16, blue: 0.23).opacity(0.85) : Color(red: 0.95, green: 0.96, blue: 0.98)
    }

    private var inputBackgroundColor: Color {
        activeColorScheme == .dark ? Color(red: 0.08, green: 0.11, blue: 0.18) : Color.white
    }

    private var borderColor: Color {
        activeColorScheme == .dark ? Color.white.opacity(0.12) : Color(red: 0.88, green: 0.91, blue: 0.94)
    }

    private func handlePrimaryAction() {
        if manager.isOnline {
            Task { await manager.logout() }
        } else {
            Task { await manager.login(password: password) }
        }
    }

    private func refreshStatus() {
        isRefreshing = true
        Task {
            await manager.checkStatus()
            monitor.refreshInterfaces()
            try? await Task.sleep(nanoseconds: 600_000_000)
            isRefreshing = false
        }
    }

    private func loadSavedPassword() {
        if let saved = KeychainService.loadPassword(forAccount: manager.config.username) {
            self.password = saved
        }
    }
}
