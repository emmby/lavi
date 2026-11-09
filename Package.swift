// swift-tools-version: 5.9
import PackageDescription
import Foundation

let packageDir = URL(fileURLWithPath: #filePath).deletingLastPathComponent().path
let CandidateLibDirs = ["\(packageDir)/target/debug", "\(packageDir)/target/release"]
var linkerFlags: [String] = []
for dir in CandidateLibDirs where FileManager.default.fileExists(atPath: dir) {
    linkerFlags.append(contentsOf: ["-L", dir])
}
if linkerFlags.isEmpty {
    linkerFlags.append(contentsOf: ["-L", "\(packageDir)/target/debug"])
}
linkerFlags.append("-llavi_c_cpp")

let package = Package(
    name: "Lavi",
    platforms: [
        .macOS(.v11),
        .iOS(.v13),
        .tvOS(.v13),
        .watchOS(.v6)
    ],
    products: [
        .library(
            name: "Lavi",
            targets: ["Lavi"]
        )
    ],
    targets: [
        .target(
            name: "CLavi",
            path: "lavi-swift/Sources/CLavi",
            publicHeadersPath: "include",
            cSettings: [
                .headerSearchPath("../../../lavi-c-cpp/include")
            ],
            linkerSettings: [
                .unsafeFlags(linkerFlags)
            ]
        ),
        .target(
            name: "Lavi",
            dependencies: ["CLavi"],
            path: "lavi-swift/Sources/Lavi"
        ),
        .testTarget(
            name: "LaviTests",
            dependencies: ["Lavi"],
            path: "lavi-swift/Tests/LaviTests"
        )
    ]
)
