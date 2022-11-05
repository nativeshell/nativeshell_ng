# flutter_engine_context

Flutter plugin that provides access to Flutter engine components (like view or texture registrar) from native code.

## Example

Dart code:
```dart
    final handle = await FlutterEngineContext.instance.getEngineHandle();
    // pass the handle native code (i.e. through FFI).
    nativeMethod(handle);
```

Rust code:
```rust
    let context = FlutterEngineContext::new();
    let flutter_view = context.get_flutter_view(handle);
    let texture_registry = contet.get_texture_registry(handle);
```

Rust code for Android:
```rust
    let context = FlutterEngineContext::new(&jni_env, class_loader);
    let flutter_view = context.get_flutter_view(handle);
    let texture_registry = contet.get_texture_registry(handle);
```

On Android the `FlutterEngineContext` needs to be initialized with JNI environment and class loader used to load Flutter plugin (or application code).
