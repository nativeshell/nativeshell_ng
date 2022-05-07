use core_foundation::runloop::{CFRunLoopGetMain, CFRunLoopRef};
use objc::{
    class,
    declare::ClassDecl,
    runtime::{method_exchangeImplementations, Class, Method, Sel},
    sel, sel_impl,
};
use once_cell::sync::Lazy;
use std::cell::Cell;

static mut FAKE_MAIN_THREAD: usize = 0;

extern "C" {
    static mut _CFMainPThread: usize;
    fn _CFRunLoopSetCurrent(roop: CFRunLoopRef);
    fn pthread_self() -> usize;
    fn class_getClassMethod(cls: *const Class, sel: Sel) -> *mut Method;
}

extern "C" fn is_main_thread(_class: &Class, _sel: Sel) -> bool {
    return unsafe { FAKE_MAIN_THREAD == pthread_self() };
}

static NS_THREAD_REPLACEMENT: Lazy<&'static Class> = Lazy::new(|| unsafe {
    let superclass = class!(NSObject);
    let mut decl = ClassDecl::new("IMFakeNSThread", superclass).unwrap();
    decl.add_class_method(
        sel![isMainThread],
        is_main_thread as extern "C" fn(&Class, Sel) -> bool,
    );
    decl.register()
});

thread_local! {
    static ALREADY_DONE : Cell<bool> = Cell::new(false);
}

/// NSApplication is braindead and insist on running on main thread. Unfortunataly
/// Rust test harness is already blocking main thread so we need some swizzling
/// to convince the NSApplication that this is main thread.
//
/// This mostly works, except for main dispatch queue messages. Those are only
/// pumped when `pthread_main_np()` returns 1, and to the best of my knowledge
/// there's no way to work around that.
///
/// That said, this should be good enough for basic unit tests.
pub fn ensure_ns_app_thinks_it_is_main_thread() {
    let already_done = ALREADY_DONE.with(|v| v.replace(true));
    if !already_done {
        unsafe {
            FAKE_MAIN_THREAD = pthread_self();
            let m1 = class_getClassMethod(class!(NSThread), sel!(isMainThread));
            let m2 = class_getClassMethod(*NS_THREAD_REPLACEMENT, sel!(isMainThread));
            method_exchangeImplementations(m1, m2);
            _CFRunLoopSetCurrent(CFRunLoopGetMain());
            _CFMainPThread = pthread_self();
        }
    }
}
