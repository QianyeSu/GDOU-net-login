import Foundation
import CommonCrypto

public enum SrunCrypto {
    private static let base64Alphabet = Array("LVoJPiCN2R8G90yg+hmFHuacZ1OWMnrsSTXkYpUq/3dlbfKwv6xztjI7DeBE45QA")

    public static func fkbase64(_ data: [UInt8]) -> String {
        var result = ""
        let len = data.count
        var i = 0
        while i < len {
            let b0 = Int(data[i])
            let b1 = i + 1 < len ? Int(data[i + 1]) : 0
            let b2 = i + 2 < len ? Int(data[i + 2]) : 0

            let idx0 = (b0 >> 2) & 0x3F
            let idx1 = ((b0 & 0x03) << 4) | ((b1 >> 4) & 0x0F)
            let idx2 = ((b1 & 0x0F) << 2) | ((b2 >> 6) & 0x03)
            let idx3 = b2 & 0x3F

            result.append(base64Alphabet[idx0])
            result.append(base64Alphabet[idx1])
            if i + 1 < len {
                result.append(base64Alphabet[idx2])
            } else {
                result.append("=")
            }
            if i + 2 < len {
                result.append(base64Alphabet[idx3])
            } else {
                result.append("=")
            }
            i += 3
        }
        return result
    }

    public static func mix(_ buffer: [UInt8], appendSize: Bool) -> [UInt32] {
        var res: [UInt32] = []
        let chunks = stride(from: 0, to: buffer.count, by: 4).map {
            Array(buffer[$0..<min($0 + 4, buffer.count)])
        }
        for chunk in chunks {
            var bytes = [UInt8](repeating: 0, count: 4)
            for j in 0..<chunk.count {
                bytes[j] = chunk[j]
            }
            let val = UInt32(bytes[0]) | (UInt32(bytes[1]) << 8) | (UInt32(bytes[2]) << 16) | (UInt32(bytes[3]) << 24)
            res.append(val)
        }
        if appendSize {
            res.append(UInt32(buffer.count))
        }
        return res
    }

    public static func split(_ buffer: [UInt32], includeSize: Bool) -> [UInt8] {
        let len = buffer.count
        if len == 0 { return [] }
        let sizeRecord = buffer[len - 1]
        if includeSize {
            let size = UInt32((len - 1) * 4)
            if sizeRecord < (size >= 3 ? size - 3 : 0) || sizeRecord > size {
                return []
            }
        }
        var bytes: [UInt8] = []
        for val in buffer {
            bytes.append(UInt8(val & 0xFF))
            bytes.append(UInt8((val >> 8) & 0xFF))
            bytes.append(UInt8((val >> 16) & 0xFF))
            bytes.append(UInt8((val >> 24) & 0xFF))
        }
        if includeSize {
            let targetLen = Int(sizeRecord)
            if targetLen <= bytes.count {
                bytes = Array(bytes[0..<targetLen])
            }
        }
        return bytes
    }

    public static func xencode(msg: String, key: String) -> [UInt8] {
        let msgBytes = Array(msg.utf8)
        if msgBytes.isEmpty { return [] }
        var mixedMsg = mix(msgBytes, appendSize: true)
        let mixedKey = mix(Array(key.utf8), appendSize: false)
        let len = mixedMsg.count
        let last = len - 1
        var right = mixedMsg[last]
        let c: UInt32 = 0x9e3779b9
        var d: UInt32 = 0
        let count = 6 + 52 / len

        for _ in 0..<count {
            d = d &+ c
            let e = (d >> 2) & 3
            for p in 0...last {
                let left = mixedMsg[(p + 1) % len]
                let term1 = ((right >> 5) ^ (left << 2))
                let term2 = ((left >> 3) ^ (right << 4)) ^ (d ^ left)
                let keyIdx = Int((UInt32(p) & 3) ^ e)
                let kVal = keyIdx < mixedKey.count ? mixedKey[keyIdx] : 0
                let term3 = kVal ^ right

                right = term1 &+ term2 &+ term3 &+ mixedMsg[p]
                mixedMsg[p] = right
            }
        }
        return split(mixedMsg, includeSize: false)
    }

    public static func hmacMd5Hex(password: String, token: String) -> String {
        let pBytes = Array(password.utf8)
        let tBytes = Array(token.utf8)
        var result = [UInt8](repeating: 0, count: Int(CC_MD5_DIGEST_LENGTH))
        CCHmac(CCHmacAlgorithm(kCCHmacAlgMD5), tBytes, tBytes.count, pBytes, pBytes.count, &result)
        return result.map { String(format: "%02x", $0) }.joined()
    }

    public static func sha1Hex(_ input: String) -> String {
        let bytes = Array(input.utf8)
        var digest = [UInt8](repeating: 0, count: Int(CC_SHA1_DIGEST_LENGTH))
        CC_SHA1(bytes, CC_LONG(bytes.count), &digest)
        return digest.map { String(format: "%02x", $0) }.joined()
    }
}
