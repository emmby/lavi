package io.lavi;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.file.*;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.Locale;

public final class NativeLoader {
    private static boolean loaded = false;

    private NativeLoader() {}

    public static synchronized void load() {
        if (loaded) return;

        // 1. Check -Dlavi.library.path
        String customPath = System.getProperty("lavi.library.path");
        if (customPath != null && !customPath.trim().isEmpty()) {
            String libName = getNativeLibName(System.getProperty("os.name"));
            Path candidate = Paths.get(customPath, libName);
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
        } catch (UnsatisfiedLinkError ignored) {
            // Continue to bundled extraction
        } catch (SecurityException ignored) {
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
            byte[] libBytes;
            try {
                ByteArrayOutputStream baos = new ByteArrayOutputStream();
                byte[] buffer = new byte[8192];
                int read;
                while ((read = is.read(buffer)) != -1) {
                    baos.write(buffer, 0, read);
                }
                libBytes = baos.toByteArray();
            } catch (IOException e) {
                throw new UnsatisfiedLinkError("Failed to read bundled native library resource: " + e.getMessage());
            } finally {
                try {
                    is.close();
                } catch (IOException ignored) {}
            }

            String versionHash = NativeLoader.class.getPackage() != null
                ? NativeLoader.class.getPackage().getImplementationVersion()
                : null;
            if (versionHash == null || versionHash.trim().isEmpty() || "dev".equals(versionHash)) {
                try {
                    MessageDigest md = MessageDigest.getInstance("SHA-256");
                    byte[] digest = md.digest(libBytes);
                    StringBuilder sb = new StringBuilder();
                    for (byte b : digest) {
                        sb.append(String.format("%02x", b & 0xff));
                    }
                    versionHash = sb.substring(0, 16);
                } catch (NoSuchAlgorithmException e) {
                    versionHash = "dev";
                }
            }

            String laviTmp = System.getProperty("lavi.tmpdir");
            Path baseDir;
            if (laviTmp != null && !laviTmp.trim().isEmpty()) {
                baseDir = Paths.get(laviTmp);
            } else {
                baseDir = Paths.get(System.getProperty("java.io.tmpdir"));
            }

            Path extracted = null;
            try {
                extracted = extractOrReuse(new ByteArrayInputStream(libBytes), versionHash, baseDir, libName);
                System.load(extracted.toAbsolutePath().toString());
                loaded = true;
                return;
            } catch (Throwable t) {
                // Fallback to user cache dir if baseDir is noexec or write-failed
                try {
                    Path userCache = Paths.get(System.getProperty("user.home"), ".cache", "lavi");
                    extracted = extractOrReuse(new ByteArrayInputStream(libBytes), versionHash, userCache, libName);
                    System.load(extracted.toAbsolutePath().toString());
                    loaded = true;
                    return;
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

        String normalizedArch;
        if (lowerArch.equals("arm64") || lowerArch.equals("aarch64")) {
            normalizedArch = "aarch64";
        } else if (lowerArch.equals("amd64") || lowerArch.equals("x86_64") || lowerArch.equals("x64")) {
            normalizedArch = "x86_64";
        } else {
            normalizedArch = lowerArch;
        }

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
        try (InputStream in = stream) {
            Files.copy(in, tempFile, StandardCopyOption.REPLACE_EXISTING);
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
            if (Files.exists(Paths.get("/etc/alpine-release"))) return true;
            Path lib = Paths.get("/lib");
            if (Files.isDirectory(lib)) {
                try (java.util.stream.Stream<Path> s = Files.list(lib)) {
                    if (s.anyMatch(p -> p.getFileName().toString().startsWith("ld-musl-"))) return true;
                }
            }
        } catch (Throwable ignored) {}
        return false;
    }
}
