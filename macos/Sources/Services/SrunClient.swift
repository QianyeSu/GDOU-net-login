import Foundation
import Darwin

public final class SrunClient {
    public var config: AppConfig
    private let session: URLSession
    private let probeSession: URLSession

    public init(config: AppConfig) {
        self.config = config

        // Keep authentication requests reasonably forgiving, but do not let a
        // captive-portal probe block the UI for one timeout per candidate.  The
        // Windows client uses a short probe timeout for the same reason.
        let sessionConfig = URLSessionConfiguration.ephemeral
        sessionConfig.timeoutIntervalForRequest = 8.0
        sessionConfig.timeoutIntervalForResource = 10.0
        sessionConfig.waitsForConnectivity = false
        sessionConfig.requestCachePolicy = .reloadIgnoringLocalAndRemoteCacheData
        sessionConfig.connectionProxyDictionary = [:] // Portal traffic must bypass the system proxy/TUN.

        let probeConfig = URLSessionConfiguration.ephemeral
        probeConfig.timeoutIntervalForRequest = 1.5
        probeConfig.timeoutIntervalForResource = 2.0
        probeConfig.waitsForConnectivity = false
        probeConfig.requestCachePolicy = .reloadIgnoringLocalAndRemoteCacheData
        probeConfig.connectionProxyDictionary = [:]

        self.session = URLSession(configuration: sessionConfig, delegate: NoRedirectDelegate(), delegateQueue: nil)
        self.probeSession = URLSession(configuration: probeConfig, delegate: NoRedirectDelegate(), delegateQueue: nil)
    }

    // MARK: - Login

    public func login(password: String) async throws -> String {
        guard !config.username.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw SrunError.custom("请先填写校园网账号")
        }
        guard !password.isEmpty else {
            throw SrunError.custom("请先填写认证密码")
        }

        let detected = (try? await probePortalFast()) ?? PortalProbe()
        let (portalUrl, acid, ip) = try await resolveLoginContext(detected: detected)

        // Do not submit a second login if the account is already online.  This
        // also makes repeated clicks idempotent on portals that keep the first
        // request pending for a short time.
        if let state = try? await getLoginState(portalUrl: portalUrl),
           state.error.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() == "ok" {
            return "账号已经在线"
        }

        let token = try await getChallenge(portalUrl: portalUrl, ip: ip, acid: acid)
        let hmd5 = SrunCrypto.hmacMd5Hex(password: password, token: token)

        let infoPayload: [String: String] = [
            "username": config.username,
            "password": password,
            "ip": ip,
            "acid": String(acid),
            "enc_ver": "srun_bx1"
        ]
        let infoData = try JSONSerialization.data(withJSONObject: infoPayload, options: [])
        let infoJson = String(data: infoData, encoding: .utf8) ?? ""
        let encodedInfo = SrunCrypto.fkbase64(SrunCrypto.xencode(msg: infoJson, key: token))
        let info = "{SRBX1}\(encodedInfo)"

        let chksumRaw = "\(token)\(config.username)\(token)\(hmd5)\(token)\(acid)\(token)\(ip)\(token)\(config.n)\(token)\(config.loginType)\(token)\(info)"
        let chksum = SrunCrypto.sha1Hex(chksumRaw)

        let ts = Int64(Date().timeIntervalSince1970 * 1000)
        let callback = "jQuery1124_\(ts)"

        var components = URLComponents(string: "\(cleanPortalUrl(portalUrl))/cgi-bin/srun_portal")!
        components.queryItems = [
            URLQueryItem(name: "callback", value: callback),
            URLQueryItem(name: "action", value: "login"),
            URLQueryItem(name: "username", value: config.username),
            URLQueryItem(name: "password", value: "{MD5}\(hmd5)"),
            URLQueryItem(name: "os", value: config.osName),
            URLQueryItem(name: "name", value: config.deviceName),
            URLQueryItem(name: "double_stack", value: "0"),
            URLQueryItem(name: "chksum", value: chksum),
            URLQueryItem(name: "info", value: info),
            URLQueryItem(name: "ac_id", value: String(acid)),
            URLQueryItem(name: "ip", value: ip),
            URLQueryItem(name: "n", value: String(config.n)),
            URLQueryItem(name: "type", value: String(config.loginType)),
            URLQueryItem(name: "_", value: String(ts))
        ]

        guard let requestUrl = components.url else {
            throw SrunError.custom("Invalid request URL components")
        }

        var request = URLRequest(url: requestUrl)
        request.httpMethod = "GET"

        let (data, response) = try await session.data(for: request)
        guard let httpResponse = response as? HTTPURLResponse, httpResponse.statusCode == 200 else {
            throw SrunError.custom("Login request failed with invalid HTTP response")
        }

        let rawText = String(data: data, encoding: .utf8) ?? ""
        let parsed = try parsePortalResponse(rawText)

        if parsed["error"] != "ok" {
            let msg = portalErrorMessage(parsed: parsed, rawText: rawText)
            throw SrunError.custom("登录未成功（Portal=\(portalUrl), ac_id=\(acid), IP=\(ip)）：\(msg)")
        }

        return parsed["suc_msg"] ?? parsed["res"] ?? "登录成功"
    }

    // MARK: - Logout

    public func logout() async throws -> String {
        let detected = (try? await probePortalFast()) ?? PortalProbe()
        let portalUrl = try resolvePortalUrl(detected: detected)
        let acid = try await resolveAcid(detected: detected)
        let ip: String
        if let state = try? await getLoginState(portalUrl: portalUrl),
           let onlineIp = state.onlineIp,
           isUsableIPv4(onlineIp) {
            ip = onlineIp
        } else {
            ip = try resolveUserIP(detected: detected)
        }

        let ts = Int64(Date().timeIntervalSince1970 * 1000)
        let callback = "jQuery1124_\(ts)"

        var components = URLComponents(string: "\(cleanPortalUrl(portalUrl))/cgi-bin/srun_portal")!
        components.queryItems = [
            URLQueryItem(name: "callback", value: callback),
            URLQueryItem(name: "action", value: "logout"),
            URLQueryItem(name: "username", value: config.username),
            URLQueryItem(name: "ac_id", value: String(acid)),
            URLQueryItem(name: "ip", value: ip),
            URLQueryItem(name: "os", value: config.osName),
            URLQueryItem(name: "name", value: config.deviceName),
            URLQueryItem(name: "_", value: String(ts))
        ]

        guard let requestUrl = components.url else {
            throw SrunError.custom("Invalid logout URL")
        }

        let (data, _) = try await session.data(for: URLRequest(url: requestUrl))
        let rawText = String(data: data, encoding: .utf8) ?? ""
        let parsed = try parsePortalResponse(rawText)

        if parsed["error"] != "ok" {
            let msg = portalErrorMessage(parsed: parsed, rawText: rawText)
            throw SrunError.custom("登出失败: \(msg)")
        }

        return "已成功断开连接"
    }

    // MARK: - Status & State

    public func probeOnlineStatus() async -> OnlineStatusAssessment {
        let detected = (try? await probePortalFast()) ?? PortalProbe()
        guard let portalUrl = try? resolvePortalUrl(detected: detected) else {
            // Cannot resolve portal, check internet
            let net = await probeInternetOnline()
            return OnlineStatusAssessment(
                status: net ? .online : .offline,
                detail: net ? "Portal 未识别，但外网正常连通" : "Portal 未识别且网络不可达"
            )
        }

        do {
            let state = try await getLoginState(portalUrl: portalUrl)
            let err = state.error.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
            if err == "ok" {
                return OnlineStatusAssessment(status: .online, detail: state.userName.map { "用户: \($0)" })
            } else if err == "not_online" || err == "notonline" || err == "offline" {
                return OnlineStatusAssessment(status: .offline, detail: "校园网处于离线状态")
            } else {
                let net = await probeInternetOnline()
                return OnlineStatusAssessment(
                    status: net ? .online : .offline,
                    detail: net ? "Portal 响应异常，但外网正常连通" : "离线: \(err)"
                )
            }
        } catch {
            let net = await probeInternetOnline()
            return OnlineStatusAssessment(
                status: net ? .online : .offline,
                detail: net ? "Portal 心跳未通，但外网可访问" : "网络连接异常: \(error.localizedDescription)"
            )
        }
    }

    public func getLoginState(portalUrl: String) async throws -> LoginState {
        let ts = Int64(Date().timeIntervalSince1970 * 1000)
        let callback = "jQuery1124_\(ts)"
        var components = URLComponents(string: "\(cleanPortalUrl(portalUrl))/cgi-bin/rad_user_info")!
        components.queryItems = [
            URLQueryItem(name: "callback", value: callback),
            URLQueryItem(name: "_", value: String(ts))
        ]

        // Status is a best-effort heartbeat, not an authentication request;
        // use the short probe timeout so the UI can recover from an unplugged
        // network quickly.
        let (data, _) = try await probeSession.data(for: URLRequest(url: components.url!))
        let raw = String(data: data, encoding: .utf8) ?? ""
        let jsonStr = stripJsonp(raw)
        guard let jsonData = jsonStr.data(using: .utf8),
              let state = try? JSONDecoder().decode(LoginState.self, from: jsonData) else {
            throw SrunError.custom("Failed to parse rad_user_info response")
        }
        return state
    }

    // MARK: - Challenge

    public func getChallenge(portalUrl: String, ip: String, acid: UInt32) async throws -> String {
        let ts = Int64(Date().timeIntervalSince1970 * 1000)
        let callback = "jQuery1124_\(ts)"

        var components = URLComponents(string: "\(cleanPortalUrl(portalUrl))/cgi-bin/get_challenge")!
        components.queryItems = [
            URLQueryItem(name: "callback", value: callback),
            URLQueryItem(name: "username", value: config.username),
            URLQueryItem(name: "ip", value: ip),
            URLQueryItem(name: "ac_id", value: String(acid)),
            URLQueryItem(name: "_", value: String(ts))
        ]

        let (data, response) = try await session.data(for: URLRequest(url: components.url!))
        guard let httpResponse = response as? HTTPURLResponse, httpResponse.statusCode == 200 else {
            throw SrunError.custom("Challenge request failed")
        }

        let raw = String(data: data, encoding: .utf8) ?? ""
        let jsonStr = stripJsonp(raw)
        guard let jsonData = jsonStr.data(using: .utf8),
              let dict = (try? JSONSerialization.jsonObject(with: jsonData)) as? [String: Any] else {
            throw SrunError.custom("Invalid challenge json: \(raw)")
        }

        if let challenge = dict["challenge"] as? String, !challenge.isEmpty {
            return challenge
        }

        let error = dict["error"] as? String ?? "unknown"
        let msg = dict["error_msg"] as? String ?? ""
        throw SrunError.custom("Challenge error: \(error) \(msg)")
    }

    // MARK: - Portal Auto Probing

    public func probePortalFast() async throws -> PortalProbe {
        var targets = [
            config.portalUrl.isEmpty ? nil : config.portalUrl,
            config.probeUrl.isEmpty ? nil : config.probeUrl,
            "http://172.16.200.11/",
            "http://192.168.0.1/",
            "http://www.msftconnecttest.com/connecttest.txt",
            "http://neverssl.com/"
        ].compactMap { $0 }.compactMap(normalizedRequestURL)

        // A captive portal often lives at the current subnet gateway and does
        // not redirect the Microsoft probe on macOS.  Add a few local-gateway
        // candidates without touching the user's routing table.
        for origin in localGatewayCandidates().prefix(3) where !targets.contains(origin) {
            targets.append(origin)
        }

        // Probes are independent.  Running them concurrently keeps login
        // bounded by the short probe timeout instead of adding one timeout per
        // unreachable gateway (a common source of the “一直连接” symptom).
        let results = await withTaskGroup(of: (Int, PortalProbe?).self, returning: [(Int, PortalProbe?)].self) { group in
            for (index, target) in targets.enumerated() {
                group.addTask { [self] in
                    (index, await self.probeSingleTarget(target))
                }
            }
            var collected: [(Int, PortalProbe?)] = []
            for await result in group {
                collected.append(result)
            }
            return collected
        }
        for (_, probe) in results.sorted(by: { $0.0 < $1.0 }) {
            guard let probe, probe.portalUrl != nil else { continue }
            return probe
        }
        return PortalProbe()
    }

    public func probeDetailedTraces() async -> [PortalProbeTrace] {
        var targets = [
            config.portalUrl.isEmpty ? nil : config.portalUrl,
            config.probeUrl.isEmpty ? nil : config.probeUrl,
            "http://172.16.200.11/",
            "http://192.168.0.1/",
            "http://www.msftconnecttest.com/connecttest.txt",
            "http://neverssl.com/"
        ].compactMap { $0 }.compactMap(normalizedRequestURL)
        for origin in localGatewayCandidates().prefix(3) where !targets.contains(origin) {
            targets.append(origin)
        }

        var traces: [PortalProbeTrace] = []
        for target in targets {
            let trace = await probeSingleTrace(target)
            traces.append(trace)
        }
        return traces
    }

    private func probeSingleTarget(_ target: String) async -> PortalProbe? {
        guard let url = URL(string: target) else { return nil }
        var request = URLRequest(url: url)
        request.httpMethod = "GET"

        do {
            let (data, response) = try await probeSession.data(for: request)
            if let http = response as? HTTPURLResponse {
                // Check 302 redirect location
                if let location = http.value(forHTTPHeaderField: "Location") {
                    let resolved = URL(string: location, relativeTo: url)?.absoluteURL.absoluteString ?? location
                    if let probe = parseProbeFromUrl(resolved) {
                        return probe
                    }
                }
                // Check body text for portal indicators
                let body = String(data: data, encoding: .utf8) ?? ""
                if let probe = parseProbeFromBody(body) {
                    return probe
                }
                if body.range(of: "(?:srun_portal|get_challenge|rad_user_info|ac_id)", options: .regularExpression) != nil,
                   let origin = originString(from: url) {
                    return PortalProbe(portalUrl: origin)
                }
            }
        } catch {
            // Ignore error in fast probe
        }
        return nil
    }

    private func probeSingleTrace(_ target: String) async -> PortalProbeTrace {
        guard let url = URL(string: target) else {
            return PortalProbeTrace(target: target, error: "Invalid URL")
        }
        var request = URLRequest(url: url)
        request.httpMethod = "GET"

        do {
            let (data, response) = try await probeSession.data(for: request)
            if let http = response as? HTTPURLResponse {
                let locationHeader = http.value(forHTTPHeaderField: "Location")
                let location = locationHeader.flatMap { URL(string: $0, relativeTo: url)?.absoluteURL.absoluteString ?? $0 }
                let body = String(data: data, encoding: .utf8) ?? ""
                let probe = location.flatMap { parseProbeFromUrl($0) } ?? parseProbeFromBody(body)
                let isInternet = (http.statusCode == 200 && (body.contains("Microsoft Connect Test") || target.contains("neverssl")))

                return PortalProbeTrace(
                    target: target,
                    status: http.statusCode,
                    location: location,
                    error: nil,
                    portalUrl: probe?.portalUrl,
                    acid: probe?.acid,
                    userIp: probe?.userIp,
                    confirmsInternet: isInternet
                )
            }
        } catch {
            return PortalProbeTrace(target: target, error: error.localizedDescription)
        }
        return PortalProbeTrace(target: target, error: "No response")
    }

    private func probeInternetOnline() async -> Bool {
        let targets = [config.probeUrl, "http://www.msftconnecttest.com/connecttest.txt", "http://neverssl.com/"]
            .filter { !$0.isEmpty }
        for target in targets {
            guard let normalized = normalizedRequestURL(target), let url = URL(string: normalized) else { continue }
            var request = URLRequest(url: url)
            request.httpMethod = "GET"
            if let (data, response) = try? await probeSession.data(for: request),
               let http = response as? HTTPURLResponse, http.statusCode == 200 {
                let text = String(data: data, encoding: .utf8) ?? ""
                if text.contains("Microsoft Connect Test") || normalized.contains("neverssl") {
                    return true
                }
            }
        }
        return false
    }

    // MARK: - Helpers & Resolvers

    private func resolveLoginContext(detected: PortalProbe) async throws -> (String, UInt32, String) {
        let portalUrl = try resolvePortalUrl(detected: detected)
        let acid = try await resolveAcid(detected: detected)
        // `rad_user_info` only contains a useful IP after a successful login.
        // Using its empty value (or 0.0.0.0) for the first login is a common
        // reason a macOS client appears to stay in “connecting”.  Prefer the
        // portal-provided IP, then an explicit bind/current IP, and fail with
        // an actionable message instead of sending an invalid SRUN request.
        let ip = try resolveUserIP(detected: detected)
        return (portalUrl, acid, ip)
    }

    private func resolveUserIP(detected: PortalProbe) throws -> String {
        // A saved bind/user IP belongs to a particular DHCP lease.  Campus
        // Wi-Fi commonly changes the third octet after roaming, so accepting
        // that value blindly makes SRUN reject an otherwise valid password.
        // Validate explicit values against the interfaces that exist right
        // now, and only use a stale saved value as a last-resort fallback when
        // interface enumeration itself temporarily fails.
        let currentAddresses = localIPv4Addresses()
        // The first physical address is the interface macOS currently
        // advertises for this host (the same ordering used by the main UI).
        // Put it before persisted values: a saved address can still exist on
        // another connected adapter while no longer being the campus-network
        // egress.  The advanced settings page remains available for users who
        // need a deliberate multi-interface override.
        let candidates: [String?]
        if config.bindIpExplicit == true {
            // A deliberate selection in 高级网卡 is an opt-in override.  It
            // is still checked against the interfaces below so a removed
            // adapter cannot be submitted accidentally.
            candidates = [config.bindIp, currentAddresses.first, detected.userIp, config.userIp]
        } else {
            candidates = [currentAddresses.first, detected.userIp, config.userIp, config.bindIp]
        }

        for value in candidates.compactMap({ $0?.trimmingCharacters(in: .whitespacesAndNewlines) }) {
            if isUsableIPv4(value), currentAddresses.contains(value) {
                return value
            }
        }

        // If getifaddrs failed transiently, retain the old explicit setting so
        // the caller receives the Portal's real response rather than an
        // unrelated "cannot determine IP" error.  Normally this branch is not
        // reached and stale DHCP addresses are never submitted.
        if currentAddresses.isEmpty {
            for value in candidates.compactMap({ $0?.trimmingCharacters(in: .whitespacesAndNewlines) }) {
                if isUsableIPv4(value) { return value }
            }
        }
        throw SrunError.custom("无法确定本机校园网 IPv4 地址，请关闭 TUN/代理后刷新，或在设置中填写本机 IP")
    }

    private func isUsableIPv4(_ value: String) -> Bool {
        let octets = value.split(separator: ".").compactMap { Int($0) }
        guard octets.count == 4, octets.allSatisfy({ (0...255).contains($0) }) else { return false }
        if octets[0] == 0 || octets[0] == 127 || (octets[0] == 169 && octets[1] == 254) {
            return false
        }
        return true
    }

    private func localGatewayCandidates() -> [String] {
        var results: [String] = []
        for ipString in localIPv4Addresses() {
            let parts = ipString.split(separator: ".").compactMap { Int($0) }
            guard parts.count == 4 else { continue }
            let candidates = [
                "http://\(parts[0]).\(parts[1]).\(parts[2]).1/",
                "http://\(parts[0]).\(parts[1]).0.1/",
                "http://\(parts[0]).129.1.1/"
            ]
            for candidate in candidates where !results.contains(candidate) {
                results.append(candidate)
            }
        }
        return results
    }

    private func localIPv4Addresses() -> [String] {
        var values: [(name: String, ip: String)] = []
        var ifap: UnsafeMutablePointer<ifaddrs>?
        guard getifaddrs(&ifap) == 0, let first = ifap else { return [] }
        defer { freeifaddrs(ifap) }

        var current: UnsafeMutablePointer<ifaddrs>? = first
        while let item = current {
            let interface = item.pointee
            let flags = interface.ifa_flags
            if (flags & UInt32(IFF_UP)) != 0,
               (flags & UInt32(IFF_LOOPBACK)) == 0,
               let address = interface.ifa_addr,
               address.pointee.sa_family == UInt8(AF_INET) {
                var host = [CChar](repeating: 0, count: Int(NI_MAXHOST))
                if getnameinfo(address, socklen_t(address.pointee.sa_len), &host, socklen_t(host.count), nil, 0, NI_NUMERICHOST) == 0 {
                    let ip = String(cString: host)
                    if isUsableIPv4(ip) {
                        values.append((String(cString: interface.ifa_name), ip))
                    }
                }
            }
            current = interface.ifa_next
        }

        // Wi-Fi/Ethernet interfaces are preferred over virtual adapters.  Keep
        // the remaining addresses as a fallback for USB Ethernet and docks.
        let physical = values.filter { $0.name.hasPrefix("en") }.map(\.ip)
        let other = values.filter { !$0.name.hasPrefix("en") }.map(\.ip)
        var unique: [String] = []
        for ip in physical + other where !unique.contains(ip) {
            unique.append(ip)
        }
        return unique
    }

    private func resolvePortalUrl(detected: PortalProbe) throws -> String {
        if !config.portalUrl.isEmpty {
            return cleanPortalUrl(config.portalUrl)
        }
        if let detectedUrl = detected.portalUrl, !detectedUrl.isEmpty {
            return cleanPortalUrl(detectedUrl)
        }
        // Known default for GDOU if network redirect fails
        return "http://172.16.200.11"
    }

    private func resolveAcid(detected: PortalProbe) async throws -> UInt32 {
        if let acid = config.acid {
            return acid
        }
        if let detectedAcid = detected.acid {
            return detectedAcid
        }
        if config.autoQueryAcid {
            // GDOU's current portal uses ac_id=1 when a redirect does not
            // expose the value.  Restrict this compatibility fallback to that
            // known portal instead of silently using 1 for another school.
            let portal = (try? resolvePortalUrl(detected: detected)) ?? ""
            if portal.contains("172.16.200.11") {
                return 1
            }
            throw SrunError.custom("无法自动识别 ac_id，请在设置中填写当前 Portal 地址或手动指定 ac_id")
        }
        throw SrunError.custom("未设置 ac_id；请在设置中开启自动识别或填写 ac_id")
    }

    private func cleanPortalUrl(_ url: String) -> String {
        var trimmed = url.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmed.starts(with: "http://") && !trimmed.starts(with: "https://") {
            trimmed = "http://\(trimmed)"
        }
        guard var components = URLComponents(string: trimmed), let scheme = components.scheme, let host = components.host else {
            return trimmed.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        }
        // Users commonly paste the browser URL (`/index_1.html?...`).  SRUN
        // endpoints live at the origin, so carrying that path would produce a
        // silently invalid `/index_1.html/cgi-bin/...` request.
        if components.path.isEmpty || components.path == "/" || components.path.range(of: "index(?:_\\d+)?\\.(?:html?|php)", options: .regularExpression) != nil {
            components.path = ""
            components.query = nil
            components.fragment = nil
        }
        let port = components.port.map { ":\($0)" } ?? ""
        return "\(scheme)://\(host)\(port)"
    }

    private func normalizedRequestURL(_ value: String) -> String? {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        let candidate = (trimmed.hasPrefix("http://") || trimmed.hasPrefix("https://")) ? trimmed : "http://\(trimmed)"
        return URL(string: candidate) == nil ? nil : candidate
    }

    private func originString(from url: URL) -> String? {
        guard let scheme = url.scheme, let host = url.host else { return nil }
        let port = url.port.map { ":\($0)" } ?? ""
        return "\(scheme)://\(host)\(port)"
    }

    private func parsePortalResponse(_ text: String) throws -> [String: String] {
        let jsonStr = stripJsonp(text)
        guard let data = jsonStr.data(using: .utf8),
              let dict = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw SrunError.custom("Cannot parse portal json: \(text)")
        }
        var result: [String: String] = [:]
        for (k, v) in dict {
            result[k] = "\(v)"
        }
        return result
    }

    private func portalErrorMessage(parsed: [String: String], rawText: String) -> String {
        let candidates = [parsed["error_msg"], parsed["res"], parsed["error"]]
            .compactMap { $0?.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty && $0.lowercased() != "<null>" && $0.lowercased() != "null" }
        if let message = candidates.first {
            return message
        }
        let compact = rawText.trimmingCharacters(in: .whitespacesAndNewlines)
        if compact.isEmpty {
            return "Portal 未返回错误说明"
        }
        let preview = String(compact.prefix(220))
        return "Portal 未返回错误说明（原始响应：\(preview)）"
    }

    private func stripJsonp(_ text: String) -> String {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let open = trimmed.firstIndex(of: "("),
              let close = trimmed.lastIndex(of: ")"),
              close > open else {
            return trimmed
        }
        let start = trimmed.index(after: open)
        return String(trimmed[start..<close])
    }

    private func parseProbeFromUrl(_ urlStr: String) -> PortalProbe? {
        guard let components = URLComponents(string: urlStr) else { return nil }
        var probe = PortalProbe()
        if let scheme = components.scheme, let host = components.host {
            let portStr = components.port.map { ":\($0)" } ?? ""
            probe.portalUrl = "\(scheme)://\(host)\(portStr)"
        }
        if let items = components.queryItems {
            for item in items {
                if item.name == "ac_id" || item.name == "acid", let val = item.value, let num = UInt32(val) {
                    probe.acid = num
                }
                if item.name == "user_ip" || item.name == "ip" || item.name == "wlanuserip", let val = item.value {
                    probe.userIp = val
                }
            }
        }
        // Also check /index_1.html, /index_17 and similar portal paths.
        if probe.acid == nil, let path = components.url?.path,
           let range = path.range(of: "index[_-]?(\\d+)", options: .regularExpression) {
            let sub = path[range]
            let digits = sub.filter { $0.isNumber }
            probe.acid = UInt32(digits)
        }
        return probe
    }

    private func parseProbeFromBody(_ body: String) -> PortalProbe? {
        var probe = PortalProbe()

        // Some portals return a relative redirect or only JavaScript metadata;
        // do not require an absolute URL before extracting ac_id/user_ip.
        if let url = firstRegexCapture(#"(https?://[A-Za-z0-9._:-]+(?:/[^\s'\"<>)]*)?)"#, in: body),
           let fromURL = parseProbeFromUrl(url) {
            probe = fromURL
        }
        if probe.acid == nil,
           let value = firstRegexCapture(#"(?:ac_id|acid)\s*[=:]\s*[\"']?(\d+)"#, in: body) {
            probe.acid = UInt32(value)
        }
        if probe.userIp == nil {
            probe.userIp = firstRegexCapture(#"(?:user_ip|wlanuserip|ip)\s*[=:]\s*[\"']?((?:\d{1,3}\.){3}\d{1,3})"#, in: body)
        }
        if probe.portalUrl == nil,
           let url = firstRegexCapture(#"(https?://[A-Za-z0-9._:-]+)"#, in: body) {
            probe.portalUrl = parseProbeFromUrl(url)?.portalUrl
        }
        return (probe.portalUrl != nil || probe.acid != nil || probe.userIp != nil) ? probe : nil
    }

    private func firstRegexCapture(_ pattern: String, in text: String) -> String? {
        guard let regex = try? NSRegularExpression(pattern: pattern, options: [.caseInsensitive]) else {
            return nil
        }
        let range = NSRange(text.startIndex..<text.endIndex, in: text)
        guard let match = regex.firstMatch(in: text, options: [], range: range), match.numberOfRanges > 1,
              let captureRange = Range(match.range(at: 1), in: text) else {
            return nil
        }
        return String(text[captureRange])
    }
}

// MARK: - Delegates and Errors

private final class NoRedirectDelegate: NSObject, URLSessionTaskDelegate {
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse, newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) {
        // Stop automatic redirects to capture Location header
        completionHandler(nil)
    }
}

public enum SrunError: LocalizedError {
    case custom(String)

    public var errorDescription: String? {
        switch self {
        case .custom(let msg): return msg
        }
    }
}
