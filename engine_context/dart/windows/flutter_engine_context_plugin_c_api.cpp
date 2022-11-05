#include "include/flutter_engine_context/flutter_engine_context_plugin_c_api.h"

#include <flutter/plugin_registrar_windows.h>

#include "flutter_engine_context_plugin.h"

void FlutterEngineContextPluginCApiRegisterWithRegistrar(
    FlutterDesktopPluginRegistrarRef registrar) {
  engine_context::FlutterEngineContextPlugin::RegisterWithRegistrar(
      flutter::PluginRegistrarManager::GetInstance()
          ->GetRegistrar<flutter::PluginRegistrarWindows>(registrar),
      registrar);
}

size_t FlutterEngineContextGetFlutterView(int64_t engine_handle) {
  return engine_context::GetFlutterView(engine_handle);
}

FlutterDesktopTextureRegistrarRef
FlutterEngineContextGetTextureRegistrar(int64_t engine_handle) {
  return engine_context::GetTextureRegistrar(engine_handle);
}

FlutterDesktopMessengerRef
FlutterEngineContextGetBinaryMessenger(int64_t engine_handle) {
  return engine_context::GetBinaryMessenger(engine_handle);
}