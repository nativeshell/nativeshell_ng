#import "EngineContextPlugin.h"

@interface _FlutterEngineContext : NSObject {
@public
  __weak NSView *flutterView;
@public
  __weak id<FlutterBinaryMessenger> binaryMessenger;
@public
  __weak id<FlutterTextureRegistry> textureRegistry;
}
@end

@implementation _FlutterEngineContext

@end

@interface FlutterEngineContextPlugin () {
  int64_t engineHandle;
}
@end

@implementation FlutterEngineContextPlugin

static NSMutableDictionary *registry;
static int64_t nextHandle = 1;

+ (void)initialize {
  registry = [NSMutableDictionary new];
}

+ (void)registerWithRegistrar:(NSObject<FlutterPluginRegistrar> *)registrar {
  FlutterEngineContextPlugin *instance =
      [[FlutterEngineContextPlugin alloc] init];
  instance->engineHandle = nextHandle;
  ++nextHandle;

  // View is available only after registerWithRegistrar: completes. And we don't
  // want to keep strong reference to the registrar in instance because it
  // references engine and unfortunately instance itself will leak given current
  // Flutter plugin architecture on macOS;
  dispatch_async(dispatch_get_main_queue(), ^{
    _FlutterEngineContext *context = [_FlutterEngineContext new];
    context->flutterView = registrar.view;
    context->binaryMessenger = registrar.messenger;
    context->textureRegistry = registrar.textures;
    // There is no unregister callback on macOS, which means we'll leak
    // an _FlutterEngineContext instance for every engine. Fortunately the
    // instance is tiny and only uses weak pointers to reference engine
    // artifacts.
    [registry setObject:context forKey:@(instance->engineHandle)];
  });

  FlutterMethodChannel *channel = [FlutterMethodChannel
      methodChannelWithName:@"dev.nativeshell.flutter_engine_context"
            binaryMessenger:[registrar messenger]];
  [registrar addMethodCallDelegate:instance channel:channel];
}

- (void)handleMethodCall:(FlutterMethodCall *)call
                  result:(FlutterResult)result {
  if ([@"getEngineHandle" isEqualToString:call.method]) {
    result(@(engineHandle));
  } else {
    result(FlutterMethodNotImplemented);
  }
}

+ (NSView *)getFlutterView:(int64_t)engineHandle {
  _FlutterEngineContext *context = [registry objectForKey:@(engineHandle)];
  return context->flutterView;
}

+ (id<FlutterTextureRegistry>)getTextureRegistry:(int64_t)engineHandle {
  _FlutterEngineContext *context = [registry objectForKey:@(engineHandle)];
  return context->textureRegistry;
}

+ (id<FlutterBinaryMessenger>)getBinaryMessenger:(int64_t)engineHandle {
  _FlutterEngineContext *context = [registry objectForKey:@(engineHandle)];
  return context->binaryMessenger;
}

@end
