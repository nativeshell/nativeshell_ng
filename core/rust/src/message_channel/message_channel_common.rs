use std::{cell::Ref, fmt::Display};

use once_cell::sync::OnceCell;

use crate::{Context, IsolateId, MessageChannel, Value, RunLoopSender};

#[derive(Debug)]
pub enum SendMessageError {
    InvalidIsolate,
    MessageRefused,
    IsolateShutDown,
    ChannelNotFound { channel: String },
    HandlerNotRegistered { channel: String },
}

#[derive(Debug)]
pub enum PostMessageError {
    InvalidIsolate,
    MessageRefused,
}

impl Display for SendMessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidIsolate => write!(f, "target isolate not found"),
            Self::MessageRefused => write!(f, "target isolate refused the message"),
            Self::IsolateShutDown => {
                write!(f, "target isolate was shut down while waiting for response")
            }
            Self::ChannelNotFound { channel } => {
                write!(f, "message channel \"{}\" not found", channel)
            }
            Self::HandlerNotRegistered { channel } => {
                write!(
                    f,
                    "message handler for channel \"{}\" not registered",
                    channel
                )
            }
        }
    }
}

impl Display for PostMessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidIsolate => write!(f, "target isolate not found"),
            Self::MessageRefused => write!(f, "target isolate refused the message"),
        }
    }
}

impl std::error::Error for SendMessageError {}
impl std::error::Error for PostMessageError {}

pub trait MessageChannelDelegate {
    fn on_isolate_joined(&self, isolate: IsolateId);
    fn on_message(&self, isolate: IsolateId, message: Value, reply: Box<dyn FnOnce(Value) -> bool>);
    fn on_isolate_exited(&self, isolate: IsolateId);
}

pub trait GetMessageChannel {
    fn message_channel(&self) -> Ref<MessageChannel>;
}

impl GetMessageChannel for Context {
    fn message_channel(&self) -> Ref<MessageChannel> {
        self.get_attachment(MessageChannel::new)
    }
}

pub(crate) static RUN_LOOP_SENDER: OnceCell<RunLoopSender> = OnceCell::new();
