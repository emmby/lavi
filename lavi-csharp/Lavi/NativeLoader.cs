using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace Lavi;

internal static class NativeLoader
{
    internal const string LibraryName = "lavi_c_cpp";
    private static int _initialized;
    private static IntPtr _cachedHandle;

    internal static void EnsureLoaded()
    {
        if (Interlocked.Exchange(ref _initialized, 1) == 0)
        {
            NativeLibrary.SetDllImportResolver(typeof(NativeLoader).Assembly, Resolve);
        }
    }

    private static string PlatformFileName()
    {
        if (RuntimeInformation.IsOSPlatform(OSPlatform.OSX))
        {
            return "liblavi_c_cpp.dylib";
        }
        if (RuntimeInformation.IsOSPlatform(OSPlatform.Windows))
        {
            return "lavi_c_cpp.dll";
        }
        return "liblavi_c_cpp.so";
    }

    private static IntPtr Resolve(string libraryName, Assembly assembly, DllImportSearchPath? searchPath)
    {
        if (libraryName != LibraryName)
        {
            return IntPtr.Zero;
        }

        if (_cachedHandle != IntPtr.Zero)
        {
            return _cachedHandle;
        }

        var envPath = Environment.GetEnvironmentVariable("LAVI_LIB_PATH");
        if (!string.IsNullOrWhiteSpace(envPath) && File.Exists(envPath))
        {
            if (NativeLibrary.TryLoad(envPath, out var envHandle))
            {
                _cachedHandle = envHandle;
                return envHandle;
            }
        }

        var fileName = PlatformFileName();
        var candidates = new List<string>();

        void AddSearchRoots(string? startDir)
        {
            var current = startDir;
            for (var depth = 0; depth < 7 && !string.IsNullOrEmpty(current); depth++)
            {
                candidates.Add(Path.Combine(current, "native", fileName));
                candidates.Add(Path.Combine(current, "lavi-csharp", "native", fileName));
                candidates.Add(Path.Combine(current, "target", "debug", fileName));
                candidates.Add(Path.Combine(current, "target", "release", fileName));
                current = Directory.GetParent(current)?.FullName;
            }
        }

        AddSearchRoots(AppContext.BaseDirectory);
        AddSearchRoots(Directory.GetCurrentDirectory());

        foreach (var candidate in candidates)
        {
            if (File.Exists(candidate) && NativeLibrary.TryLoad(candidate, out var handle))
            {
                _cachedHandle = handle;
                return handle;
            }
        }

        if (NativeLibrary.TryLoad(libraryName, assembly, searchPath, out var defaultHandle))
        {
            _cachedHandle = defaultHandle;
            return defaultHandle;
        }

        return IntPtr.Zero;
    }
}
