// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "GDOU-net-login",
    platforms: [
        .macOS(.v15)
    ],
    products: [
        .executable(
            name: "GDOU-net-login",
            targets: ["GDOU-net-login"]
        )
    ],
    dependencies: [],
    targets: [
        .executableTarget(
            name: "GDOU-net-login",
            dependencies: [],
            path: "Sources"
        )
    ]
)
