use std::ffi::c_void;

mod async_method_handler;
mod event_channel;
mod method_handler;

#[cfg(not(feature = "mock"))]
mod codec;
#[cfg(not(feature = "mock"))]
mod message_channel;
#[cfg(not(feature = "mock"))]
mod native_vector;

#[cfg(feature = "mock")]
#[path = "mock_message_channel.rs"]
mod message_channel;

pub use async_method_handler::*;
pub use event_channel::*;
pub use message_channel::*;
pub use method_handler::*;

/// Type alias for isolate identifier
pub type IsolateId = i64;

#[repr(C)]
#[cfg(not(feature = "mock"))]
struct MessageChannelContext {
    size: isize,
    ffi_data: *mut c_void,
    register_isolate: *mut c_void,
    send_message: *mut c_void,
    attach_weak_persistent_handle: *mut c_void,
    update_persistant_handle_size: *mut c_void,

    allocate_vec_i8: *mut c_void,
    allocate_vec_u8: *mut c_void,
    allocate_vec_i16: *mut c_void,
    allocate_vec_u16: *mut c_void,
    allocate_vec_i32: *mut c_void,
    allocate_vec_u32: *mut c_void,
    allocate_vec_i64: *mut c_void,
    allocate_vec_f32: *mut c_void,
    allocate_vec_f64: *mut c_void,
    free_vec_i8: *mut c_void,
    free_vec_u8: *mut c_void,
    free_vec_i16: *mut c_void,
    free_vec_u16: *mut c_void,
    free_vec_i32: *mut c_void,
    free_vec_u32: *mut c_void,
    free_vec_i64: *mut c_void,
    free_vec_f32: *mut c_void,
    free_vec_f64: *mut c_void,
    resize_vec_u8: *mut c_void,
}

#[repr(u64)]
pub enum FunctionResult {
    NoError = 0,
    InvalidStructSize = 1,
}

#[no_mangle]
#[inline(never)]
#[cfg(not(feature = "mock"))]
pub extern "C" fn nativeshell_init_message_channel_context(data: *mut c_void) -> FunctionResult {
    use crate::{
        ffi::nativeshell_init_ffi, finalizable_handle_native::attach_weak_persistent_handle,
        finalizable_handle_native::update_persistent_handle_size,
    };

    use self::native_vector::*;

    let context = data as *mut MessageChannelContext;
    let context = unsafe { &mut *context };
    if context.size != std::mem::size_of::<MessageChannelContext>() as isize {
        println!("Bad struct size");
        return FunctionResult::InvalidStructSize;
    }
    nativeshell_init_ffi(context.ffi_data);
    context.register_isolate = register_isolate as *mut _;
    context.send_message = post_message as *mut _;
    context.attach_weak_persistent_handle = attach_weak_persistent_handle as *mut _;
    context.update_persistant_handle_size = update_persistent_handle_size as *mut _;
    context.allocate_vec_i8 = allocate_vec_i8 as *mut _;
    context.allocate_vec_u8 = allocate_vec_u8 as *mut _;
    context.allocate_vec_i16 = allocate_vec_i16 as *mut _;
    context.allocate_vec_i16 = allocate_vec_u16 as *mut _;
    context.allocate_vec_i32 = allocate_vec_i32 as *mut _;
    context.allocate_vec_u32 = allocate_vec_u32 as *mut _;
    context.allocate_vec_i64 = allocate_vec_i64 as *mut _;
    context.allocate_vec_f32 = allocate_vec_f32 as *mut _;
    context.allocate_vec_f64 = allocate_vec_f64 as *mut _;
    context.free_vec_i8 = free_vec_i8 as *mut _;
    context.free_vec_u8 = free_vec_u8 as *mut _;
    context.free_vec_i16 = free_vec_i16 as *mut _;
    context.free_vec_u16 = free_vec_u16 as *mut _;
    context.free_vec_i32 = free_vec_i32 as *mut _;
    context.free_vec_u32 = free_vec_u32 as *mut _;
    context.free_vec_i64 = free_vec_i64 as *mut _;
    context.free_vec_f32 = free_vec_f32 as *mut _;
    context.free_vec_f64 = free_vec_f64 as *mut _;
    context.resize_vec_u8 = resize_vec_u8 as *mut _;

    FunctionResult::NoError
}

#[no_mangle]
#[inline(never)]
#[cfg(feature = "mock")]
pub extern "C" fn nativeshell_init_message_channel_context(_data: *mut c_void) -> FunctionResult {
    FunctionResult::NoError
}
