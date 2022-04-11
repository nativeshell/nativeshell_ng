import 'dart:async';
import 'dart:ffi';
import 'dart:isolate';
import 'dart:typed_data';

import 'codec.dart';
import 'finalizable_handle.dart';
import 'native_functions.dart';

class NoSuchChannelException implements Exception {
  NoSuchChannelException({required this.channel});

  @override
  String toString() => 'Native MessageChannel "$channel" not found';

  final String channel;
}

typedef MessageChannelHandler = FutureOr<dynamic> Function(dynamic message);

class MessageChannel {
  MessageChannel(this.name, {MessageChannelContext? context})
      : _context = context ?? MessageChannelContext.getDefault() {
    _context._channels[name] = this;
  }

  void setHandler(MessageChannelHandler? handler) {
    this.handler = handler;
  }

  Future<dynamic> sendMessage(dynamic message) {
    return _context._sendMessage(name, message);
  }

  MessageChannelHandler? handler;
  final String name;
  final MessageChannelContext _context;
}

class MessageChannelContextError implements Exception {
  const MessageChannelContextError(this.message);

  final String message;

  @override
  String toString() => message;
}

// IsolateId is determined by taking address of this object.
final _isolateIdObject = Object();

class MessageChannelContext implements FinalizableHandleProvider {
  MessageChannelContext._(this.functions) {
    _init();
  }

  static MessageChannelContext getDefault() {
    final functions = NativeFunctions.getDefault();
    return _forFunctions(functions);
  }

  /// Returns MessageChannelContext for given FFI function. The function must
  /// call 'nativeshell_init_message_channel_context' with given argument
  /// and return the result.
  /// This is necessary to do in Flutter plugins where each plugin may have its
  /// own context and thus should have uniquely named init function.
  static MessageChannelContext forInitFunction(
      Pointer<Void> messageChannelInitFunction) {
    final function = messageChannelInitFunction
        .cast<NativeFunction<Int64 Function(Pointer<Void>)>>();
    return _forFunctions(NativeFunctions.get(function));
  }

  static MessageChannelContext _forFunctions(NativeFunctions functions) {
    for (final c in _contexts) {
      if (c.functions.token == functions.token) {
        return c;
      }
    }
    final res = MessageChannelContext._(functions);
    _contexts.add(res);
    return res;
  }

  void _init() {
    final port = RawReceivePort(_onMessage);
    isolateId =
        functions.registerIsolate(port.sendPort.nativePort, _isolateIdObject);
    if (isolateId == -1) {
      throw const MessageChannelContextError(
          "NativeShell Rust Context not initialized. "
          "Please initialize context using nativeshell_core::Context::new() "
          "before callind dart code.");
    }
  }

  Future<dynamic> _sendMessage(String channel, dynamic message) async {
    final replyId = _nextReplyId++;
    _postMessage(["message", replyId, channel, message]);
    final completer = Completer();
    _pendingReplies[replyId] = completer;
    return completer.future;
  }

  void _postMessage(Object? message) {
    final data = Serializer(functions).serialize(message);
    functions.postMessage(isolateId, data.data, data.length);
  }

  static final _contexts = <MessageChannelContext>{};

  void _handleMessage(List data) async {
    final message = data[0] as String;
    if (message == "reply") {
      final replyId = data[1] as int;
      final value = data[2];
      final completer = _pendingReplies.remove(replyId)!;
      completer.complete(value);
    } else if (message == "reply_no_channel") {
      final replyId = data[1] as int;
      final channel = data[2] as String;
      final completer = _pendingReplies.remove(replyId)!;
      completer.completeError(NoSuchChannelException(channel: channel));
    } else if (message == "send_message") {
      final channelName = data[1] as String;
      final replyId = data[2] as int;
      final value = data[3];
      final channel = _channels[channelName];
      if (channel == null) {
        _postMessage(["no_channel", replyId, channelName]);
      } else {
        final handler = channel.handler;
        if (handler == null) {
          _postMessage(["no_handler", replyId, channelName]);
        } else {
          final result = await handler(value);
          _postMessage(["reply", replyId, result]);
        }
      }
    } else if (message == "post_message") {
      // like send message but result is ignored
      final channelName = data[1] as String;
      final value = data[2];
      final channel = _channels[channelName];
      final handler = channel?.handler;
      if (handler != null) {
        handler(value);
      }
    }
  }

  void _onMessage(dynamic message) {
    if (message is SendPort) {
      Isolate.current
          .addOnExitListener(message, response: ['isolate_exit', isolateId]);
    } else {
      if (message is List) {
        final d = message.last as Uint8List;
        final data = ByteData.view(d.buffer, d.offsetInBytes, d.length);
        final v = Deserializer().deserialize(data, message, this);
        _handleMessage(v as List);
      }
    }
  }

  int _nextReplyId = 0;
  final _pendingReplies = <int, Completer<dynamic>>{};
  final _channels = <String, MessageChannel>{};
  late final IsolateId isolateId;
  final NativeFunctions functions;

  @override
  FinalizableHandle? getFinalizableHandle(int id) {
    final handle = FinalizableHandle(id);
    // Let native code override the return value in case there already is one
    return functions.attachWeakPersistentHandle(handle, id, null, isolateId)
        as FinalizableHandle?;
  }
}
