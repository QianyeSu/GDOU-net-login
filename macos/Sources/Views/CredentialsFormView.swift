import SwiftUI

public struct CredentialsFormView: View {
    @ObservedObject var manager: AutoReconnectManager
    @State private var password: String = ""
    @State private var showPassword: Bool = false
    @State private var selectedIspIndex: Int = 0

    private let ispSuffixes = [
        "",             // 校园网直连
        "@telecom",     // 中国电信
        "@cmcc",        // 中国移动
        "@unicom"       // 中国联通
    ]
    private let ispLabels = ["校园网", "电信", "移动", "联通"]

    public var body: some View {
        VStack(spacing: 14) {
            // Account Input
            VStack(alignment: .leading, spacing: 6) {
                Text("校园网账号 / 学号")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundColor(.secondary)

                HStack {
                    Image(systemName: "person.fill")
                        .font(.system(size: 13))
                        .foregroundColor(.secondary)
                        .frame(width: 20)

                    TextField("请输入学号或账号", text: $manager.config.username)
                        .textFieldStyle(.plain)
                        .font(.system(size: 13))
                        .onChange(of: manager.config.username) { _, _ in
                            manager.updateConfig(manager.config)
                            loadSavedPassword()
                        }
                }
                .padding(.horizontal, 10)
                .frame(height: 36)
                .background(Color.primary.opacity(0.04))
                .cornerRadius(8)
                .overlay(
                    RoundedRectangle(cornerRadius: 8)
                        .stroke(Color.primary.opacity(0.1), lineWidth: 1)
                )
            }

            // Password Input
            VStack(alignment: .leading, spacing: 6) {
                Text("密码")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundColor(.secondary)

                HStack {
                    Image(systemName: "lock.fill")
                        .font(.system(size: 13))
                        .foregroundColor(.secondary)
                        .frame(width: 20)

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
                            .font(.system(size: 12))
                            .foregroundColor(.secondary)
                    }
                    .buttonStyle(.plain)
                }
                .padding(.horizontal, 10)
                .frame(height: 36)
                .background(Color.primary.opacity(0.04))
                .cornerRadius(8)
                .overlay(
                    RoundedRectangle(cornerRadius: 8)
                        .stroke(Color.primary.opacity(0.1), lineWidth: 1)
                )
            }

            // ISP Quick Selector Segment
            VStack(alignment: .leading, spacing: 6) {
                Text("运营商选择")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundColor(.secondary)

                HStack(spacing: 6) {
                    ForEach(0..<ispLabels.count, id: \.self) { idx in
                        Button(action: {
                            applyIspSelection(index: idx)
                        }) {
                            Text(ispLabels[idx])
                                .font(.system(size: 12, weight: selectedIspIndex == idx ? .semibold : .regular))
                                .frame(maxWidth: .infinity)
                                .frame(height: 28)
                                .background(
                                    selectedIspIndex == idx ?
                                    Color.blue.opacity(0.2) :
                                    Color.primary.opacity(0.04)
                                )
                                .foregroundColor(selectedIspIndex == idx ? .blue : .primary)
                                .cornerRadius(6)
                                .overlay(
                                    RoundedRectangle(cornerRadius: 6)
                                        .stroke(selectedIspIndex == idx ? Color.blue.opacity(0.5) : Color.clear, lineWidth: 1)
                                )
                        }
                        .buttonStyle(.plain)
                    }
                }
            }

            // Primary Action Button (Connect / Disconnect)
            Button(action: {
                handlePrimaryAction()
            }) {
                HStack(spacing: 8) {
                    if manager.isConnecting {
                        ProgressView()
                            .scaleEffect(0.8)
                    } else {
                        Image(systemName: manager.isOnline ? "wifi.slash" : "bolt.fill")
                    }
                    Text(manager.isOnline ? "断开连接" : "一键连接")
                }
                .liquidGlassButton(isPrimary: !manager.isOnline, isDanger: manager.isOnline)
            }
            .buttonStyle(.plain)
            .disabled(manager.isConnecting || (!manager.isOnline && (manager.config.username.isEmpty || password.isEmpty)))
            .opacity((!manager.isOnline && (manager.config.username.isEmpty || password.isEmpty)) ? 0.6 : 1.0)
            .padding(.top, 4)

            // Toggles
            VStack(spacing: 8) {
                Toggle(isOn: Binding(
                    get: { manager.config.autoReconnect },
                    set: { val in
                        manager.config.autoReconnect = val
                        manager.updateConfig(manager.config)
                    }
                )) {
                    HStack {
                        Image(systemName: "arrow.triangle.2.circlepath")
                            .foregroundColor(.blue)
                        Text("开启自动重连 (离线自动重试)")
                            .font(.system(size: 12))
                    }
                }
                .toggleStyle(.checkbox)
                .frame(maxWidth: .infinity, alignment: .leading)

                Toggle(isOn: Binding(
                    get: { manager.config.startupEnabled },
                    set: { val in
                        manager.config.startupEnabled = val
                        _ = LaunchAtLogin.setEnabled(val)
                        manager.updateConfig(manager.config)
                    }
                )) {
                    HStack {
                        Image(systemName: "power")
                            .foregroundColor(.purple)
                        Text("开机自启动并在后台守护")
                            .font(.system(size: 12))
                    }
                }
                .toggleStyle(.checkbox)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .padding(.top, 2)
        }
        .padding(16)
        .liquidGlassCard(cornerRadius: 14)
        .onAppear {
            loadSavedPassword()
            detectIspFromUsername()
        }
    }

    private func handlePrimaryAction() {
        if manager.isOnline {
            Task {
                await manager.logout()
            }
        } else {
            Task {
                await manager.login(password: password)
            }
        }
    }

    private func loadSavedPassword() {
        if let saved = KeychainService.loadPassword(forAccount: manager.config.username) {
            self.password = saved
        }
    }

    private func detectIspFromUsername() {
        let u = manager.config.username
        if u.contains("@telecom") {
            selectedIspIndex = 1
        } else if u.contains("@cmcc") {
            selectedIspIndex = 2
        } else if u.contains("@unicom") {
            selectedIspIndex = 3
        } else {
            selectedIspIndex = 0
        }
    }

    private func applyIspSelection(index: Int) {
        selectedIspIndex = index
        var baseUsername = manager.config.username
        for suffix in ispSuffixes where !suffix.isEmpty {
            if let range = baseUsername.range(of: suffix) {
                baseUsername.removeSubrange(range)
            }
        }
        let targetSuffix = ispSuffixes[index]
        manager.config.username = baseUsername + targetSuffix
        manager.updateConfig(manager.config)
        loadSavedPassword()
    }
}
