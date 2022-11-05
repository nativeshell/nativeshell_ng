import 'package:flutter/services.dart';

class FlutterEngineContext {
  /// Shared instance for [FlutterEngineContext].
  static final instance = FlutterEngineContext();

  final _methodChannel =
      const MethodChannel('dev.nativeshell.flutter_engine_context');

  int? _engineHandle;

  /// Returns handle for current engine. This handle can be then passed to
  /// FFI to obtain engine components (i.e. FlutterView or TextureRegistry).
  ///
  /// Dart:
  /// ```dart
  /// final handle = await FlutterEngineContext.instance.getEngineHandle();
  /// // pass the handle native code (i.e. through FFI).
  /// ```
  ///
  /// Native code:
  /// ```rust
  /// let context = FlutterEngineContext::new();
  /// let flutter_view = context.get_flutter_view(handle);
  /// let texture_registry = contet.get_texture_registry(handle);
  /// ```
  Future<int> getEngineHandle() async {
    _engineHandle ??= await _methodChannel.invokeMethod<int>('getEngineHandle');
    return _engineHandle!;
  }
}
