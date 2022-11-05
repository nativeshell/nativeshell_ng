#![allow(clippy::new_without_default)]

use std::{cell::Cell, marker::PhantomData, sync::MutexGuard};

#[cfg(target_os = "android")]
#[path = "android.rs"]
pub mod platform;

#[cfg(target_os = "windows")]
#[path = "windows.rs"]
pub mod platform;

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
pub mod platform;

#[cfg(any(target_os = "ios", target_os = "macos"))]
#[path = "darwin.rs"]
pub mod platform;

pub type FlutterEngineContextError = platform::Error;
pub type FlutterEngineContextResult<T> = Result<T, FlutterEngineContextError>;

pub type FlutterView = platform::FlutterView;
pub type FlutterTextureRegistry = platform::FlutterTextureRegistry;
pub type FlutterBinaryMessenger = platform::FlutterBinaryMessenger;
#[cfg(target_os = "android")]
pub type Activity = platform::Activity;

type PhantomUnsync = PhantomData<Cell<()>>;
type PhantomUnsend = PhantomData<MutexGuard<'static, ()>>;

pub struct FlutterEngineContext {
    platform_context: platform::PlatformContext,
    _unsync: PhantomUnsync,
    _unsend: PhantomUnsend,
}

impl FlutterEngineContext {
    /// Creates new FlutterEngineContext instance.
    /// Must be called on platform thread.
    pub fn new() -> FlutterEngineContextResult<Self> {
        Ok(Self {
            platform_context: platform::PlatformContext::new()?,
            _unsync: PhantomData,
            _unsend: PhantomData,
        })
    }

    /// Returns flutter view for given engine handle.
    pub fn get_flutter_view(
        &self,
        handle: i64,
    ) -> FlutterEngineContextResult<platform::FlutterView> {
        self.platform_context.get_flutter_view(handle)
    }

    /// Returns texture registry for given engine handle.
    pub fn get_texture_registry(
        &self,
        handle: i64,
    ) -> FlutterEngineContextResult<FlutterTextureRegistry> {
        self.platform_context.get_texture_registry(handle)
    }

    /// Returns binary messenger for given engine handle.
    pub fn get_binary_messenger(
        &self,
        handle: i64,
    ) -> FlutterEngineContextResult<FlutterBinaryMessenger> {
        self.platform_context.get_binary_messenger(handle)
    }

    /// Returns android activity for given handle.
    #[cfg(target_os = "android")]
    pub fn get_activity(&self, handle: i64) -> FlutterEngineContextResult<Activity> {
        self.platform_context.get_activity(handle)
    }
}
