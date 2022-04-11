#![allow(clippy::type_complexity)]
#![allow(clippy::new_without_default)]
#![allow(clippy::identity_op)]
#![allow(clippy::module_inception)]

mod context;
pub mod ffi;
mod finalizable_handle;
mod handle;
mod message_channel;

pub mod platform;
mod run_loop;
mod value;

// Note: Util is public but there are no API stability guarantees
pub mod util;

pub use context::*;
pub use finalizable_handle::*;
pub use handle::*;
pub use message_channel::*;
pub use run_loop::*;
pub use value::*;

#[cfg(feature = "nativeshell_derive")]
pub mod derive_internal;

#[cfg(feature = "nativeshell_derive")]
pub use nativeshell_derive::*;
