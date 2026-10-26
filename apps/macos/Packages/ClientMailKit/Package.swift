// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "ClientMailKit",
    defaultLocalization: "en",
    platforms: [.macOS(.v26)],
    products: [
        .library(name: "MailDomain", targets: ["MailDomain"]),
        .library(name: "CoreAdapter", targets: ["CoreAdapter"]),
        .library(name: "AccountsFeature", targets: ["AccountsFeature"]),
    ],
    targets: [
        // Built by scripts/build-core-bridge.sh. Run `make bridge` before building this package.
        .binaryTarget(name: "CoreBridgeFFI", path: "Artifacts/CoreBridgeFFI.xcframework"),
        .target(
            name: "CoreBridge",
            dependencies: ["CoreBridgeFFI"],
            // Generated code predates strict Swift 6 checking; isolation is enforced in CoreAdapter.
            swiftSettings: [.swiftLanguageMode(.v5)],
            // The Rust core's HTTPS client reads proxy settings (SystemConfiguration) and checks
            // certificates with the system trust store (Security). A static library cannot
            // declare these itself.
            linkerSettings: [
                .linkedFramework("SystemConfiguration"),
                .linkedFramework("Security"),
            ]
        ),
        .target(name: "MailDomain"),
        .target(name: "CoreAdapter", dependencies: ["MailDomain", "CoreBridge"]),
        .target(
            name: "AccountsFeature",
            dependencies: ["MailDomain"],
            resources: [.process("Resources")]
        ),
        .testTarget(name: "MailDomainTests", dependencies: ["MailDomain"]),
        .testTarget(name: "CoreAdapterTests", dependencies: ["CoreAdapter", "MailDomain"]),
        .testTarget(name: "AccountsFeatureTests", dependencies: ["AccountsFeature", "MailDomain"]),
    ]
)
