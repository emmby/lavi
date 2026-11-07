package io.lavi;

import java.io.IOException;
import java.io.InputStream;
import java.nio.file.*;
import java.util.Locale;

public final class NativeLoader {
    private static boolean loaded = false;

    private NativeLoader() {}

    public static synchronized void load() {
        if (loaded) return;

        // 1. Check -Dlavi.library.path
        String customPath = System.getProperty("lavi.library.path");
        if (customPath != null && !customPath.isBlank()) {
            String libName = getNativeLibName(System.getProperty("os.name"));
            Path candidate = Path.of(customPath, libName);
            if (Files.exists(candidate)) {
                try {
                    System.load(candidate.toAbsolutePath().toString());
                    loaded = true;
                    return;
                } catch (UnsatisfiedLinkError e) {
                    // Continue to next fallback
                }
            }
        }

        // 2. Try system / Android loadLibrary
        try {
            System.loadLibrary("lavi_jvm");
            loaded = true;
            return;
        } catch (UnsatisfiedLinkError | SecurityException ignored) {
            // Continue to bundled extraction
        }

        // 3. Extract from classpath resource
        String osName = System.getProperty("os.name");
        String osArch = System.getProperty("os.arch");
        boolean isMusl = isMusl();
        String target = resolveTargetArch(osName, osArch, isMusl);
        String libName = getNativeLibName(osName);

        String resourcePath = "/native/" + target + "/" + libName;
        InputStream is = NativeLoader.class.getResourceAsStream(resourcePath);
        if (is != null) {
            String versionHash = NativeLoader.class.getPackage().getImplementationVersion();
            if (versionHash == null || versionHash.isBlank()) {
                versionHash = "dev";
            }

            Path baseDir = Path.of(System.getProperty("java.io.tmpdir"));
            Path extracted = null;
            try {
                extracted = extractOrReuse(is, versionHash, baseDir, libName);
                System.load(extracted.toAbsolutePath().toString());
                loaded = true;
                return;
            } catch (Throwable t) {
                // Fallback to user cache dir if /tmp is noexec or write-failed
                try {
                    Path userCache = Path.of(System.getProperty("user.home"), ".cache", "lavi");
                    InputStream is2 = NativeLoader.class.getResourceAsStream(resourcePath);
                    if (is2 != null) {
                        extracted = extractOrReuse(is2, versionHash, userCache, libName);
                        System.load(extracted.toAbsolutePath().toString());
                        loaded = true;
                        return;
                    }
                } catch (Throwable fallbackErr) {
                    throw new UnsatisfiedLinkError("Failed to extract or load native library: " + t.getMessage()
                        + "; fallback also failed: " + fallbackErr.getMessage());
                }
            }
        }

        throw new UnsatisfiedLinkError("Could not load native library 'lavi_jvm' for os=" + osName + ", arch=" + osArch);
    }

    static String getNativeLibName(String osName) {
        String lower = osName.toLowerCase(Locale.ROOT);
        if (lower.contains("mac") || lower.contains("darwin")) {
            return "liblavi_jvm.dylib";
        } else if (lower.contains("win")) {
            return "lavi_jvm.dll";
        } else {
            return "liblavi_jvm.so";
        }
    }

    static String resolveTargetArch(String osName, String osArch, boolean isMusl) {
        String lowerOs = osName.toLowerCase(Locale.ROOT);
        String lowerArch = osArch.toLowerCase(Locale.ROOT);

        String normalizedArch = switch (lowerArch) {
            case "arm64", "aarch64" -> "aarch64";
            case "amd64", "x86_64", "x64" -> "x86_64";
            default -> lowerArch;
        };

        if (lowerOs.contains("mac") || lowerOs.contains("darwin")) {
            return "darwin-" + normalizedArch;
        } else if (lowerOs.contains("win")) {
            return "windows-" + normalizedArch;
        } else if (lowerOs.contains("linux")) {
            return "linux-" + normalizedArch + "-" + (isMusl ? "musl" : "gnu");
        } else {
            return lowerOs + "-" + normalizedArch;
        }
    }

    static Path extractOrReuse(InputStream stream, String versionHash, Path baseDir, String fileName) throws IOException {
        Path targetDir = baseDir.resolve("lavi-jni-" + versionHash);
        Path targetFile = targetDir.resolve(fileName);

        if (Files.exists(targetFile)) {
            try {
                if (Files.size(targetFile) > 0) {
                    return targetFile;
                }
            } catch (IOException ignored) {}
        }

        Files.createDirectories(targetDir);
        Path tempFile = Files.createTempFile(targetDir, "lib", ".tmp");
        try (stream) {
            Files.copy(stream, tempFile, StandardCopyOption.REPLACE_EXISTING);
        }
        try {
            Files.move(tempFile, targetFile, StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE);
        } catch (IOException e) {
            Files.move(tempFile, targetFile, StandardCopyOption.REPLACE_EXISTING);
        }
        targetFile.toFile().setExecutable(true, false);
        return targetFile;
    }

    static boolean isMusl() {
        try {
            if (Files.exists(Path.of("/etc/alpine-release"))) return true;
            Path lib = Path.of("/lib");
            if (Files.isDirectory(lib)) {
                try (var s = Files.list(lib)) {
                    if (s.anyMatch(p -> p.getFileName().toString().startsWith("ld-musl-"))) return true;
                }
            }
        } catch (Throwable ignored) {}
        return false;
    }
}
