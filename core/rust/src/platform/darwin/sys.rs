use std::{os::raw::c_char, slice};

use objc::{class, msg_send, rc::StrongPtr, sel, sel_impl};

use self::cocoa::id;

#[link(name = "Foundation", kind = "framework")]
extern "C" {}

#[cfg(target_os = "macos")]
#[link(name = "AppKit", kind = "framework")]
extern "C" {}

#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[allow(dead_code)]
#[allow(non_upper_case_globals)]

pub mod cocoa {
    use objc::{class, msg_send, runtime, sel, sel_impl};

    pub use objc::runtime::{BOOL, NO, YES};

    pub type id = *mut runtime::Object;
    pub const nil: id = 0 as id;

    #[cfg(target_pointer_width = "64")]
    pub type CGFloat = std::os::raw::c_double;
    #[cfg(not(target_pointer_width = "64"))]
    pub type CGFloat = std::os::raw::c_float;

    #[repr(C)]
    #[derive(Copy, Clone)]
    pub struct NSPoint {
        pub x: CGFloat,
        pub y: CGFloat,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    #[repr(u64)] // NSUInteger
    pub enum NSEventType {
        NSApplicationDefined = 15,
    }

    impl NSPoint {
        #[inline]
        pub fn new(x: CGFloat, y: CGFloat) -> NSPoint {
            NSPoint { x, y }
        }
    }

    pub trait NSApplication: Sized {
        unsafe fn sharedApplication(_: Self) -> id {
            msg_send![class!(NSApplication), sharedApplication]
        }
        unsafe fn activateIgnoringOtherApps_(self, ignore: BOOL);
        unsafe fn run(self);
        unsafe fn stop_(self, sender: id);
    }

    impl NSApplication for id {
        unsafe fn activateIgnoringOtherApps_(self, ignore: BOOL) {
            msg_send![self, activateIgnoringOtherApps: ignore]
        }

        unsafe fn run(self) {
            msg_send![self, run]
        }

        unsafe fn stop_(self, sender: id) {
            msg_send![self, stop: sender]
        }
    }

    #[cfg(target_pointer_width = "32")]
    type NSUInteger = std::os::raw::c_uint;

    #[cfg(target_pointer_width = "64")]
    type NSUInteger = std::os::raw::c_ulong;

    pub trait NSArray: Sized {
        unsafe fn arrayWithObjects(_: Self, objects: &[id]) -> id {
            msg_send![class!(NSArray), arrayWithObjects:objects.as_ptr()
                                    count:objects.len()]
        }

        unsafe fn count(self) -> NSUInteger;
        unsafe fn objectAtIndex(self, index: NSUInteger) -> id;
    }

    impl NSArray for id {
        unsafe fn count(self) -> NSUInteger {
            msg_send![self, count]
        }
        unsafe fn objectAtIndex(self, index: NSUInteger) -> id {
            msg_send![self, objectAtIndex: index]
        }
    }

    pub trait NSDictionary: Sized {
        unsafe fn dictionaryWithObject_forKey_(_: Self, anObject: id, aKey: id) -> id {
            msg_send![class!(NSDictionary), dictionaryWithObject:anObject forKey:aKey]
        }
        unsafe fn dictionaryWithObjects_forKeys_(_: Self, objects: id, keys: id) -> id {
            msg_send![class!(NSDictionary), dictionaryWithObjects:objects forKeys:keys]
        }
        unsafe fn keyEnumerator(self) -> id;
        unsafe fn valueForKey_(self, key: id) -> id;
    }

    impl NSDictionary for id {
        unsafe fn keyEnumerator(self) -> id {
            msg_send![self, keyEnumerator]
        }
        unsafe fn valueForKey_(self, key: id) -> id {
            msg_send![self, valueForKey: key]
        }
    }
}

const UTF8_ENCODING: usize = 4;

pub fn to_nsstring(string: &str) -> StrongPtr {
    unsafe {
        let s: id = msg_send![class!(NSString), alloc];
        let s: id = msg_send![s, initWithBytes:string.as_ptr()
                                 length:string.len()
                                 encoding:UTF8_ENCODING as id];
        StrongPtr::new(s)
    }
}

pub unsafe fn from_nsstring(ns_string: id) -> String {
    let bytes: *const c_char = msg_send![ns_string, UTF8String];
    let bytes = bytes as *const u8;

    let len = msg_send![ns_string, lengthOfBytesUsingEncoding: UTF8_ENCODING];

    let bytes = slice::from_raw_parts(bytes, len);
    std::str::from_utf8(bytes).unwrap().into()
}

pub fn to_nsdata(data: &[u8]) -> StrongPtr {
    unsafe {
        let d: id = msg_send![class!(NSData), alloc];
        let d: id = msg_send![d, initWithBytes:data.as_ptr() length:data.len()];
        StrongPtr::new(d)
    }
}

pub fn from_nsdata(data: id) -> Vec<u8> {
    unsafe {
        let bytes: *const u8 = msg_send![data, bytes];
        let length: usize = msg_send![data, length];
        let data: &[u8] = std::slice::from_raw_parts(bytes, length);
        data.into()
    }
}
