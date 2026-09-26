import Foundation
import Combine

@MainActor
public final class AutoReconnectManager: ObservableObject {
    @Published public var config: AppConfig
    @Published public var isOnline: Bool = false
    @Published public var isConnecting: Bool = false
    @Published public var statusText: String = "正在检查状态..."
    @Published public var statusDetail: String? = nil
    @Published public var lastCheckTime: Date? = nil
    @Published public var errorMessage: String? = nil
    @Published public private(set) var lastReconnectAt: Date?

    private var client: SrunClient
    private var reconnectTask: Task<Void, Never>?
    private var lastAuthAttempt: Date = .distantPast
    // A user-initiated disconnect must not be immediately undone by the
    // background guard.  The old loop could start a second login while the
    // user was clicking "立即连接", making the first request look flaky.
    private var manualDisconnect = false

    public init() {
        let loadedConfig = AppConfig.load()
        self.config = loadedConfig
        self.client = SrunClient(config: loadedConfig)
        self.lastReconnectAt = UserDefaults.standard.object(forKey: "gdou_last_reconnect_at") as? Date

        // Initial check and start loop if configured
        Task {
            await self.checkStatus()
            if self.config.autoReconnect {
                self.startLoop()
            }
        }
    }

    public func updateConfig(_ newConfig: AppConfig) {
        let oldConfig = config
        self.config = newConfig
        self.client.config = newConfig
        newConfig.save()
        if oldConfig.rememberPassword && !newConfig.rememberPassword {
            // Turning off password persistence must also remove the existing
            // Keychain item; otherwise the background loop could still reuse it.
            _ = KeychainService.deletePassword(forAccount: newConfig.username)
        }
        if newConfig.autoReconnect && (!oldConfig.autoReconnect || reconnectTask == nil) {
            startLoop()
        } else if !newConfig.autoReconnect {
            stopLoop()
        }
    }

    public func login(password: String, recordReconnect: Bool = false) async {
        guard !isConnecting else { return }

        manualDisconnect = false

        guard !config.username.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            statusText = "缺少校园网账号"
            errorMessage = "请先填写校园网账号"
            statusDetail = errorMessage
            return
        }
        guard !password.isEmpty else {
            statusText = "缺少认证密码"
            errorMessage = "请先填写认证密码"
            statusDetail = errorMessage
            return
        }

        // Cooldown check (prevent spamming the portal)
        if Date().timeIntervalSince(lastAuthAttempt) < 2.0 {
            return
        }
        lastAuthAttempt = Date()

        isConnecting = true
        statusText = "正在连接校园网..."
        errorMessage = nil

        client.config = config
        do {
            let result: String
            do {
                result = try await client.login(password: password)
            } catch {
                // SRUN can briefly retain the previous session/challenge after
                // logout and return sign_error.  Refreshing the challenge once
                // after a short delay fixes the transient case without hiding
                // ordinary bad-password/login_error responses.
                let detail = error.localizedDescription.lowercased()
                guard detail.contains("sign_error") || detail.contains("server_busy") else {
                    throw error
                }
                try await Task.sleep(nanoseconds: 1_200_000_000)
                result = try await client.login(password: password)
            }
            isOnline = true
            statusText = "网络已连接"
            statusDetail = result
            lastCheckTime = Date()
            if config.rememberPassword {
                _ = KeychainService.savePassword(password, forAccount: config.username)
            }
            if recordReconnect {
                lastReconnectAt = Date()
                UserDefaults.standard.set(lastReconnectAt, forKey: "gdou_last_reconnect_at")
            }
        } catch {
            isOnline = false
            statusText = "连接失败"
            errorMessage = error.localizedDescription
            statusDetail = error.localizedDescription
        }

        isConnecting = false
    }

    /// Run the same safe authentication path used by the background guard.
    /// The settings diagnostic calls this only after reading the opt-in
    /// Keychain item; it never displays or writes the password itself.
    public func reconnectSelfTest() async {
        guard config.rememberPassword,
              let savedPassword = KeychainService.loadPassword(forAccount: config.username),
              !savedPassword.isEmpty else {
            statusText = "重连自测无法开始"
            errorMessage = "钥匙串中没有已保存的密码；请先在主界面输入密码并登录，或开启密码保存。"
            statusDetail = errorMessage
            return
        }
        // Match the Windows diagnostic semantics: if the UI currently knows
        // the session is online, close it first and then perform a real login
        // rather than merely receiving "already online" from Portal.
        if isOnline {
            await logout()
        }
        await login(password: savedPassword, recordReconnect: true)
    }

    public func logout() async {
        guard !isConnecting else { return }
        manualDisconnect = true
        isConnecting = true
        statusText = "正在断开连接..."
        errorMessage = nil

        client.config = config
        do {
            let res = try await client.logout()
            // SRUN portals often acknowledge logout before their session table
            // is updated.  Starting challenge/login in the same millisecond
            // can produce sign_error; give the portal a short settle window.
            try? await Task.sleep(nanoseconds: 800_000_000)
            isOnline = false
            statusText = "已断开连接"
            statusDetail = res
            lastCheckTime = Date()
        } catch {
            errorMessage = error.localizedDescription
            statusText = "断开请求失败"
        }

        isConnecting = false
    }

    public func checkStatus() async {
        guard !isConnecting else { return }
        statusText = "正在检查状态..."
        errorMessage = nil
        client.config = config
        let assessment = await client.probeOnlineStatus()
        isOnline = (assessment.status == .online)
        statusText = isOnline ? "网络已连接" : "离线未连接"
        statusDetail = assessment.detail
        lastCheckTime = Date()
    }

    public func startLoop() {
        stopLoop()
        reconnectTask = Task { [weak self] in
            while !Task.isCancelled {
                guard let self = self else { break }

                let onlineCheckSec = max(self.config.onlineCheckSeconds, 15)
                let retrySec = max(self.config.retrySeconds, 5)

                if self.isOnline {
                    try? await Task.sleep(nanoseconds: UInt64(onlineCheckSec) * 1_000_000_000)
                    if Task.isCancelled { break }
                    await self.checkStatus()
                } else {
                    // Offline - check if auto-reconnect is enabled and password exists
                    if !self.manualDisconnect,
                       self.config.autoReconnect,
                       self.config.rememberPassword,
                       let savedPassword = KeychainService.loadPassword(forAccount: self.config.username),
                       !savedPassword.isEmpty {
                        await self.login(password: savedPassword, recordReconnect: true)
                    }
                    try? await Task.sleep(nanoseconds: UInt64(retrySec) * 1_000_000_000)
                }
            }
        }
    }

    public func stopLoop() {
        reconnectTask?.cancel()
        reconnectTask = nil
    }

    deinit {
        reconnectTask?.cancel()
    }
}
