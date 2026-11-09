// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "StoplightExample",
    platforms: [
        .macOS(.v11),
        .iOS(.v13)
    ],
    dependencies: [
        .package(name: "Lavi", path: "../..")
    ],
    targets: [
        .executableTarget(
            name: "StoplightExample",
            dependencies: [
                .product(name: "Lavi", package: "Lavi")
            ],
            path: "Sources"
        )
    ]
)
