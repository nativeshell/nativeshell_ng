use std::fmt::Display;

use jni::{objects::JObject, sys::jint};
use nativeshell_jni_context::AndroidJniContext;

use crate::FlutterEngineContextResult;

pub(crate) struct PlatformContext {
    java_vm: &'static jni::JavaVM,
    class_loader: jni::objects::GlobalRef,
}

#[derive(Debug)]
pub enum Error {
    InvalidHandle,
    MissingClassLoader,
    JNIError(jni::errors::Error),
    AndroidJniContextError(nativeshell_jni_context::Error),
}

pub(crate) type FlutterView = jni::objects::GlobalRef;
pub(crate) type FlutterTextureRegistry = jni::objects::GlobalRef;
pub(crate) type FlutterBinaryMessenger = jni::objects::GlobalRef;
pub(crate) type Activity = jni::objects::GlobalRef;

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::JNIError(e) => e.fmt(f),
            Error::MissingClassLoader => write!(f, "missing class loader"),
            Error::InvalidHandle => write!(f, "invalid engine handle"),
            Error::AndroidJniContextError(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for Error {}

impl From<jni::errors::Error> for Error {
    fn from(err: jni::errors::Error) -> Self {
        Error::JNIError(err)
    }
}

impl From<nativeshell_jni_context::Error> for Error {
    fn from(err: nativeshell_jni_context::Error) -> Self {
        Error::AndroidJniContextError(err)
    }
}

impl PlatformContext {
    pub fn new() -> FlutterEngineContextResult<Self> {
        let context = AndroidJniContext::get()?;
        let class_loader = context
            .class_loader()
            .cloned()
            .ok_or(Error::MissingClassLoader)?;
        Ok(Self {
            java_vm: context.vm(),
            class_loader,
        })
    }

    fn get_plugin_class<'a>(
        &'a self,
        env: &jni::JNIEnv<'a>,
    ) -> FlutterEngineContextResult<jni::objects::JClass<'a>> {
        let plugin_class = env
            .call_method(
                self.class_loader.as_obj(),
                "loadClass",
                "(Ljava/lang/String;)Ljava/lang/Class;",
                &[env
                    .new_string(
                        "dev/nativeshell/flutter_engine_context/FlutterEngineContextPlugin",
                    )?
                    .into()],
            )?
            .l()?;
        Ok(plugin_class.into())
    }

    pub fn get_activity(&self, handle: i64) -> FlutterEngineContextResult<Activity> {
        let id: jint = handle.try_into().map_err(|_| Error::InvalidHandle)?;
        let env = self.java_vm.get_env()?;
        let class = self.get_plugin_class(&env)?;
        let activity = env
            .call_static_method(
                class,
                "getActivity",
                "(I)Landroid/app/Activity;",
                &[id.into()],
            )?
            .l()?;
        if env.is_same_object(activity, JObject::null())? {
            Err(Error::InvalidHandle)
        } else {
            Ok(env.new_global_ref(activity)?)
        }
    }

    pub fn get_flutter_view(&self, handle: i64) -> FlutterEngineContextResult<FlutterView> {
        let id: jint = handle.try_into().map_err(|_| Error::InvalidHandle)?;
        let env = self.java_vm.get_env()?;
        let class = self.get_plugin_class(&env)?;
        let view = env
            .call_static_method(
                class,
                "getFlutterView",
                "(I)Lio/flutter/embedding/android/FlutterView;",
                &[id.into()],
            )?
            .l()?;
        if env.is_same_object(view, JObject::null())? {
            Err(Error::InvalidHandle)
        } else {
            Ok(env.new_global_ref(view)?)
        }
    }

    pub fn get_binary_messenger(
        &self,
        handle: i64,
    ) -> FlutterEngineContextResult<FlutterBinaryMessenger> {
        let id: jint = handle.try_into().map_err(|_| Error::InvalidHandle)?;
        let env = self.java_vm.get_env()?;
        let class = self.get_plugin_class(&env)?;
        let messenger = env
            .call_static_method(
                class,
                "getBinaryMessenger",
                "(I)Lio/flutter/plugin/common/BinaryMessenger;",
                &[id.into()],
            )?
            .l()?;
        if env.is_same_object(messenger, JObject::null())? {
            Err(Error::InvalidHandle)
        } else {
            Ok(env.new_global_ref(messenger)?)
        }
    }

    pub fn get_texture_registry(
        &self,
        handle: i64,
    ) -> FlutterEngineContextResult<FlutterTextureRegistry> {
        let id: jint = handle.try_into().map_err(|_| Error::InvalidHandle)?;
        let env = self.java_vm.get_env()?;
        let class = self.get_plugin_class(&env)?;
        let registry = env
            .call_static_method(
                class,
                "getTextureRegistry",
                "(I)Lio/flutter/view/TextureRegistry;",
                &[id.into()],
            )?
            .l()?;
        if env.is_same_object(registry, JObject::null())? {
            Err(Error::InvalidHandle)
        } else {
            Ok(env.new_global_ref(registry)?)
        }
    }
}
