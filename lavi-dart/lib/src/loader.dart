import 'dart:ffi';
import 'dart:io';
import 'dart:isolate';

DynamicLibrary? _cachedLibrary;

String _platformLibName() {
  if (Platform.isMacOS || Platform.isIOS) {
    return 'liblavi_c_cpp.dylib';
  }
  if (Platform.isWindows) {
    return 'lavi_c_cpp.dll';
  }
  return 'liblavi_c_cpp.so';
}

DynamicLibrary loadLaviLibrary() {
  final existing = _cachedLibrary;
  if (existing != null) {
    return existing;
  }

  final envPath = Platform.environment['LAVI_LIB_PATH'];
  if (envPath != null && envPath.isNotEmpty && File(envPath).existsSync()) {
    final lib = DynamicLibrary.open(envPath);
    _cachedLibrary = lib;
    return lib;
  }

  final libName = _platformLibName();
  final candidates = <String>[];

  try {
    final pkgUri = Isolate.resolvePackageUriSync(Uri.parse('package:lavi/lavi.dart'));
    if (pkgUri != null && pkgUri.scheme == 'file') {
      final libDir = File.fromUri(pkgUri).parent;
      final pkgRoot = libDir.parent;
      candidates.add('${pkgRoot.path}/native/$libName');
      candidates.add('${pkgRoot.parent.path}/target/debug/$libName');
      candidates.add('${pkgRoot.parent.path}/target/release/$libName');
    }
  } catch (_) {}

  final cwd = Directory.current.path;
  candidates.addAll([
    '$cwd/native/$libName',
    '$cwd/lavi-dart/native/$libName',
    '$cwd/../lavi-dart/native/$libName',
    '$cwd/../../lavi-dart/native/$libName',
    '$cwd/target/debug/$libName',
    '$cwd/target/release/$libName',
  ]);

  for (final path in candidates) {
    if (File(path).existsSync()) {
      final lib = DynamicLibrary.open(path);
      _cachedLibrary = lib;
      return lib;
    }
  }

  if (Platform.isIOS) {
    final lib = DynamicLibrary.process();
    _cachedLibrary = lib;
    return lib;
  }

  final lib = DynamicLibrary.open(libName);
  _cachedLibrary = lib;
  return lib;
}
