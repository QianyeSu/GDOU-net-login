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
    @Published public private(set) var lastLoginPortal: String?
    @Published public private(set) var lastLoginAcid: UInt32?
    @Published public private(set) var lastLoginIP: String?

    private var client: SrunClient
    private var reconnectTask: Task<Void, Never>?
    private var lastAuthAttempt: Date = .distantPast
    // Retain the password only for the lifetime of this process when the user
    // chooses not to persist it.  This lets the Windows-style self-test work
    // immediately after a successful manual login without writing a secret to
    // disk; a relaunch still requires the Keychain option.
    private var sessionPassword: String?

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
        if oldConfig.username != newConfig.username {
            sessionPassword = nil
        }
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
        let elapsedSinceAuth = Date().timeIntervalSince(lastAuthAttempt)
        if elapsedSinceAuth < 2.0 {
            let waitSeconds = max(1, Int(ceil(2.0 - elapsedSinceAuth)))
            statusText = "认证请求过于频繁"
            errorMessage = "请等待 \(waitSeconds) 秒后再试"
            statusDetail = errorMessage
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
                statusText = "Portal 正在同步会话，准备重试..."
                statusDetail = "已重新获取 Challenge；等待 Portal 完成上一轮会话清理"
                // Rust's reconnect_self_test waits for the logout/status
                // round-trip before logging in again.  A captive portal can
                // otherwise return sign_error for the first fresh challenge.
                // Clear a stale Portal session before the second attempt.
                // This is the part the Rust command performs through its
                // logout_once/enrich/login sequence and is essential when a
                // network drop leaves rad_user_info online for a few seconds.
                _ = try? await client.logout()
                try await Task.sleep(nanoseconds: 2_000_000_000)
                result = try await client.login(password: password)
            }
            isOnline = true
            statusText = "网络已连接"
            statusDetail = result
            lastLoginPortal = client.lastLoginPortal
            lastLoginAcid = client.lastLoginAcid
            lastLoginIP = client.lastLoginIP
            lastCheckTime = Date()
            if config.rememberPassword {
                _ = KeychainService.savePassword(password, forAccount: config.username)
            }
            sessionPassword = password
            if recordReconnect {
                lastReconnectAt = Date()
                UserDefaults.standard.set(lastReconnectAt, forKey: "gdou_last_reconnect_at")
            }
        } catch {
            isOnline = false
            statusText = "连接失败"
            lastLoginPortal = client.lastLoginPortal
            lastLoginAcid = client.lastLoginAcid
            lastLoginIP = client.lastLoginIP
            errorMessage = error.localizedDescription
            statusDetail = error.localizedDescription
        }

        isConnecting = false
    }

    /// Run the same safe authentication path used by the background guard.
    /// The settings diagnostic calls this only after reading the opt-in
    /// Keychain item; it never displays or writes the password itself.
    public func reconnectSelfTest() async {
        let savedPassword = (config.rememberPassword
            ? KeychainService.loadPassword(forAccount: config.username)
            : nil) ?? sessionPassword
        guard let savedPassword, !savedPassword.isEmpty else {
            statusText = "重连自测无法开始"
            errorMessage = "当前没有可用的密码；请先在主界面输入密码并登录，或开启钥匙串保存。"
            statusDetail = errorMessage
            return
        }

        // Match the Rust/Windows command: stop the watcher, issue a logout
        // even if the cached status is already offline, then probe/login and
        // restore the watcher in either outcome.
        stopLoop()
        await logout(resumeAutoReconnect: false)
        // Logout is best effort.  The Portal may need a short settle period,
        // and the auth cooldown also protects against a just-finished manual
        // login.  Waiting here prevents the self-test from silently returning
        // before its login request was actually allowed to run.
        let remainingCooldown = max(0, 2.1 - Date().timeIntervalSince(lastAuthAttempt))
        if remainingCooldown > 0 {
            try? await Task.sleep(nanoseconds: UInt64(remainingCooldown * 1_000_000_000))
        }
        try? await Task.sleep(nanoseconds: 400_000_000)
        await login(password: savedPassword, recordReconnect: true)
        if config.autoReconnect {
            startLoop()
        }
    }

    public func logout(resumeAutoReconnect: Bool = true) async {
        guard !isConnecting else { return }
        isConnecting = true
        statusText = "正在断开连接..."
        errorMessage = nil

        client.config = config
        do {
            let res = try await client.logout()
            // SRUN portals often acknowledge logout before their session table
            // is updated.  Starting challenge/login in the same millisecond
            // can produce sign_error; match the Rust client and give the
            // portal a full two-second settle window.
            try? await Task.sleep(nanoseconds: 2_000_000_000)
            isOnline = false
            statusText = "已断开连接"
            statusDetail = res
            lastCheckTime = Date()
        } catch {
            isOnline = false
            errorMessage = error.localizedDescription
            statusText = "断开请求失败"
        }

        isConnecting = false
        // The Rust client keeps its watcher alive after a normal logout
        // command.  The macOS port used to set a permanent manual-disconnect
        // flag here, so one click on "断开连接" silently disabled all future
        // automatic recovery.  Resume the same behavior as Windows unless a
        // reconnect self-test explicitly owns the transaction.
        if resumeAutoReconnect, config.autoReconnect {
            startLoop()
        }
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
                    if self.config.autoReconnect,
                       self.config.rememberPassword,
                       let savedPassword = KeychainService.loadPassword(forAccount: self.config.username),
                       !savedPassword.isEmpty {
                        await self.login(password: savedPassword, recordReconnect: true)
                    } else if self.config.autoReconnect,
                              let savedPassword = self.sessionPassword,
                              !savedPassword.isEmpty {
                        // A session-only password is intentionally not
                        // persisted, but it should still support reconnects
                        // until the app exits.
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
