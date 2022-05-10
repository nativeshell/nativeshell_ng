#[path = "message_channel_common.rs"]
mod common;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::{Rc, Weak},
};

pub use common::*;

use crate::{
    unpack_result, util::FutureCompleter, Context, FinalizableHandleState, IsolateId,
    MethodCallError, PlatformResult, Value,
};

pub struct MockIsolate {
    handlers: RefCell<HashMap<String, Box<dyn Fn(Value, Option<Box<dyn FnOnce(Value)>>)>>>,
}

#[derive(Debug)]
pub struct MockMethodCall {
    pub method: String,
    pub args: Value,
}

impl MockIsolate {
    pub fn new() -> Self {
        Self {
            handlers: RefCell::new(HashMap::new()),
        }
    }

    pub fn register_message_handler<F: Fn(Value, Option<Box<dyn FnOnce(Value)>>) + 'static>(
        &self,
        channel: &str,
        handler: F,
    ) {
        let mut handlers = self.handlers.borrow_mut();
        handlers.insert(channel.into(), Box::new(handler));
    }

    pub fn register_method_handler<
        F: Fn(MockMethodCall, Box<dyn FnOnce(PlatformResult)>) + 'static,
    >(
        &self,
        channel: &str,
        handler: F,
    ) {
        self.register_message_handler(channel, move |value, reply| {
            let items: Vec<Value> = value.try_into().unwrap();
            let mut items = items.into_iter();
            let call = MockMethodCall {
                method: items.next().unwrap().try_into().unwrap(),
                args: items.next().unwrap(),
            };
            handler(
                call,
                Box::new(move |res| {
                    let value = match res {
                        Ok(value) => Value::List(vec!["ok".into(), value]),
                        Err(err) => Value::List(vec![
                            "err".into(),
                            err.code.into(),
                            err.message.map(|s| s.into()).unwrap_or(Value::Null),
                            err.detail,
                        ]),
                    };
                    if let Some(reply) = reply {
                        reply(value);
                    }
                }),
            );
        });
    }

    pub fn apply(self, channel: &MessageChannel) -> Rc<RegisteredMockIsolate> {
        let isolate_id = channel.inner.register_isolate(self);
        Rc::new(RegisteredMockIsolate {
            isolate_id,
            channel: Rc::downgrade(&channel.inner),
        })
    }
}

pub struct RegisteredMockIsolate {
    isolate_id: IsolateId,
    channel: Weak<MessageChannelInner>,
}

impl RegisteredMockIsolate {
    pub fn isolate_id(&self) -> IsolateId {
        self.isolate_id
    }

    pub fn send_message<F: FnOnce(Result<Value, SendMessageError>) + 'static>(
        &self,
        channel: &str,
        message: Value,
        reply: F,
    ) {
        match self.channel.upgrade() {
            Some(message_channel) => {
                let delegates = message_channel.delegates.borrow();
                let channel = channel.to_owned();
                let delegate = delegates.get(&channel);
                match delegate {
                    Some(delegate) => {
                        delegate.on_message(
                            self.isolate_id,
                            message,
                            Box::new(move |value| {
                                reply(Ok(value));
                                true
                            }),
                        );
                    }
                    None => reply(Err(SendMessageError::ChannelNotFound { channel })),
                }
            }
            None => reply(Err(SendMessageError::MessageRefused)),
        }
    }

    pub async fn send_message_async(
        &self,
        channel: &str,
        message: Value,
    ) -> Result<Value, SendMessageError> {
        let (future, completer) = FutureCompleter::new();
        self.send_message(channel, message, move |reply| {
            completer.complete(reply);
        });
        future.await
    }

    pub fn call_method<F: FnOnce(Result<Value, MethodCallError>) + 'static>(
        &self,
        channel: &str,
        method: &str,
        argument: Value,
        reply: F,
    ) {
        let call = vec![Value::String(method.into()), argument];
        self.send_message(channel, call.into(), move |result| match result {
            Ok(value) => reply(unpack_result(value).unwrap()),
            Err(error) => reply(Err(MethodCallError::SendError(error))),
        });
    }

    pub async fn call_method_async(
        &self,
        channel: &str,
        method: &str,
        argument: Value,
    ) -> Result<Value, MethodCallError> {
        let (future, completer) = FutureCompleter::new();
        self.call_method(channel, method, argument, move |reply| {
            completer.complete(reply);
        });
        future.await
    }
}

impl Drop for RegisteredMockIsolate {
    fn drop(&mut self) {
        if let Some(channel) = self.channel.upgrade() {
            channel.unregister_isolate(self.isolate_id);
        }
    }
}

pub struct MessageChannel {
    inner: Rc<MessageChannelInner>,
}

impl MessageChannel {
    fn new() -> Self {
        RUN_LOOP_SENDER
            .set(Context::get().run_loop().new_sender())
            .map_err(|_| ())
            .expect("Message channel already initialized");
        Self {
            inner: Rc::new(MessageChannelInner {
                next_isolate: Cell::new(1),
                isolates: RefCell::new(HashMap::new()),
                delegates: RefCell::new(HashMap::new()),
            }),
        }
    }

    fn attach_finalizable_handles(value: &Value, isolate: IsolateId) {
        match value {
            Value::FinalizableHandle(value) => {
                value.attach(isolate);
            }
            Value::Map(map) => {
                for e in map.iter() {
                    Self::attach_finalizable_handles(&e.0, isolate);
                    Self::attach_finalizable_handles(&e.1, isolate);
                }
            }
            Value::List(list) => {
                for e in list {
                    Self::attach_finalizable_handles(e, isolate);
                }
            }
            _ => {}
        }
    }

    pub fn send_message<F>(
        &self,
        target_isolate: IsolateId,
        channel: &str,
        message: Value,
        reply: F,
    ) where
        F: FnOnce(Result<Value, SendMessageError>) + 'static,
    {
        let isolates = self.inner.isolates.borrow();
        let isolate = isolates.get(&target_isolate);
        match isolate {
            Some(isolate) => {
                Self::attach_finalizable_handles(&message, target_isolate);

                let handlers = isolate.handlers.borrow();
                let channel = channel.to_owned();
                let handler = handlers.get(&channel);
                match handler {
                    Some(handler) => {
                        handler(message, Some(Box::new(move |value| reply(Ok(value)))));
                    }
                    None => reply(Err(SendMessageError::ChannelNotFound { channel })),
                }
            }
            None => reply(Err(SendMessageError::InvalidIsolate)),
        }
    }

    pub fn post_message(
        &self,
        target_isolate: IsolateId,
        channel: &str,
        message: Value,
    ) -> Result<(), PostMessageError> {
        let isolates = self.inner.isolates.borrow();
        let isolate = isolates.get(&target_isolate);
        match isolate {
            Some(isolate) => {
                Self::attach_finalizable_handles(&message, target_isolate);

                let handlers = isolate.handlers.borrow();
                let channel = channel.to_owned();
                let handler = handlers.get(&channel);
                if let Some(handler) = handler {
                    handler(message, None);
                }
                Ok(())
            }
            None => Err(PostMessageError::InvalidIsolate),
        }
    }

    pub fn register_delegate<F>(&self, channel: &str, delegate: Rc<F>)
    where
        F: MessageChannelDelegate + 'static,
    {
        self.inner
            .delegates
            .borrow_mut()
            .insert(channel.into(), delegate);
    }

    pub fn unregister_delegate(&self, channel: &str) {
        self.inner.delegates.borrow_mut().remove(channel);
    }

    pub(crate) fn request_update_external_size(&self, _target_isolate: IsolateId, _handle: isize) {}
}

struct MessageChannelInner {
    next_isolate: Cell<IsolateId>,
    isolates: RefCell<HashMap<IsolateId, MockIsolate>>,
    delegates: RefCell<HashMap<String, Rc<dyn MessageChannelDelegate>>>,
}

impl MessageChannelInner {
    fn register_isolate(&self, isolate: MockIsolate) -> IsolateId {
        let isolate_id = self.next_isolate.get();
        self.next_isolate.set(isolate_id + 1);
        self.isolates.borrow_mut().insert(isolate_id, isolate);
        let delegates = self.delegates.borrow();
        for d in delegates.values() {
            d.on_isolate_joined(isolate_id);
        }
        isolate_id
    }

    fn unregister_isolate(&self, isolate: IsolateId) {
        FinalizableHandleState::get().finalize_all(isolate);
        let delegates = self.delegates.borrow();
        for d in delegates.values() {
            d.on_isolate_exited(isolate);
        }
    }
}
