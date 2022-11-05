#ifndef FLUTTER_PLUGIN_ENGINE_CONTEXT_PLUGIN_H_
#define FLUTTER_PLUGIN_ENGINE_CONTEXT_PLUGIN_H_

#include <flutter_linux/flutter_linux.h>

G_BEGIN_DECLS

#ifdef FLUTTER_PLUGIN_IMPL
#define FLUTTER_PLUGIN_EXPORT __attribute__((visibility("default")))
#else
#define FLUTTER_PLUGIN_EXPORT
#endif

typedef struct _FlutterEngineContextPlugin FlutterEngineContextPlugin;
typedef struct {
  GObjectClass parent_class;
} FlutterEngineContextPluginClass;

FLUTTER_PLUGIN_EXPORT GType flutter_engine_context_plugin_get_type();

FLUTTER_PLUGIN_EXPORT FlView *
FlutterEngineContextGetFlutterView(int64_t engine_handle);

FLUTTER_PLUGIN_EXPORT FlBinaryMessenger *
FlutterEngineContextGetBinaryMessenger(int64_t engine_handle);

FLUTTER_PLUGIN_EXPORT FlTextureRegistrar *
FlutterEngineContextGetTextureRegistrar(int64_t engine_handle);

FLUTTER_PLUGIN_EXPORT void
flutter_engine_context_plugin_register_with_registrar(
    FlPluginRegistrar *registrar);

G_END_DECLS

#endif // FLUTTER_PLUGIN_ENGINE_CONTEXT_PLUGIN_H_
