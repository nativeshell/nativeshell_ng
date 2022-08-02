use core_foundation::{
    base::{kCFAllocatorDefault, TCFType},
    date::CFAbsoluteTimeGetCurrent,
    runloop::{
        kCFRunLoopCommonModes, CFRunLoopAddSource, CFRunLoopAddTimer, CFRunLoopGetMain,
        CFRunLoopRemoveTimer, CFRunLoopRunInMode, CFRunLoopSource, CFRunLoopSourceContext,
        CFRunLoopSourceCreate, CFRunLoopSourceSignal, CFRunLoopTimer, CFRunLoopTimerContext,
        CFRunLoopTimerRef, CFRunLoopWakeUp,
    },
    string::CFStringRef,
};
use objc::rc::StrongPtr;

use std::{
    cell::Cell,
    collections::HashMap,
    ffi::c_void,
    mem::ManuallyDrop,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[cfg(target_os = "macos")]
use super::sys::cocoa::*;
#[cfg(target_os = "macos")]
use objc::{class, msg_send, sel, sel_impl};

use super::sys::to_nsstring;

pub type HandleType = usize;
pub const INVALID_HANDLE: HandleType = 0;

type Callback = Box<dyn FnOnce()>;

struct Timer {
    scheduled: Instant,
    callback: Callback,
}

struct State {
    callbacks: Vec<Callback>,
    timers: HashMap<HandleType, Timer>,
    timer: Option<CFRunLoopTimer>,
    source: Option<CFRunLoopSource>,
    run_loop_mode: StrongPtr,
}

// CFRunLoopTimer is thread safe
unsafe impl Send for State {}

struct StatePendingExecution {
    callbacks: Vec<Callback>,
    timers: Vec<Timer>,
}

//
// This is bit more complicated than just using dispatch_after and dispatch_async.
// Main reason for it is that we need to allow manual polling for scheduled events.
// This is because during window resizing neither dispatch queue nor run loop are
// running so we need to process events manually.
//

impl State {
    fn new() -> Self {
        Self {
            callbacks: Vec::new(),
            timers: HashMap::new(),
            timer: None,
            source: None,
            run_loop_mode: to_nsstring("NativeShellRunLoopMode"),
        }
    }

    fn get_pending_execution(&mut self) -> StatePendingExecution {
        let now = Instant::now();
        let pending: Vec<HandleType> = self
            .timers
            .iter()
            .filter(|v| v.1.scheduled <= now)
            .map(|v| *v.0)
            .collect();

        StatePendingExecution {
            callbacks: self.callbacks.drain(0..).collect(),
            timers: pending
                .iter()
                .map(|h| self.timers.remove(h).unwrap())
                .collect(),
        }
    }

    fn unschedule(&mut self) {
        if let Some(timer) = self.timer.take() {
            unsafe {
                CFRunLoopRemoveTimer(
                    CFRunLoopGetMain(),
                    timer.as_concrete_TypeRef(),
                    kCFRunLoopCommonModes,
                );
                CFRunLoopRemoveTimer(
                    CFRunLoopGetMain(),
                    timer.as_concrete_TypeRef(),
                    *self.run_loop_mode as CFStringRef,
                );
            };
        }
    }

    fn create_source(&mut self, state: Arc<Mutex<State>>) {
        let mutex = Arc::as_ptr(&state);
        let mut context = CFRunLoopSourceContext {
            version: 0,
            info: mutex as *mut c_void,
            retain: Some(Self::retain),
            release: Some(Self::release),
            copyDescription: None,
            equal: None,
            hash: None,
            schedule: None,
            cancel: None,
            perform: Self::on_source,
        };
        let source: CFRunLoopSource = unsafe {
            let source_ref = CFRunLoopSourceCreate(kCFAllocatorDefault, 0, &mut context as *mut _);
            TCFType::wrap_under_create_rule(source_ref)
        };
        unsafe {
            CFRunLoopAddSource(
                CFRunLoopGetMain(),
                source.as_concrete_TypeRef(),
                kCFRunLoopCommonModes,
            );
            // Register source with custom RunLoopMode. This lets clients to only process
            // events scheduled through RunLoopSender (which also includes MessageChannel).
            CFRunLoopAddSource(
                CFRunLoopGetMain(),
                source.as_concrete_TypeRef(),
                *self.run_loop_mode as CFStringRef,
            );
        }
        self.source = Some(source);
    }

    #[cfg(target_os = "macos")]
    fn destroy_source(&mut self) {
        use core_foundation::runloop::CFRunLoopRemoveSource;
        if let Some(source) = self.source.take() {
            unsafe {
                CFRunLoopRemoveSource(
                    CFRunLoopGetMain(),
                    source.as_concrete_TypeRef(),
                    kCFRunLoopCommonModes,
                );
                CFRunLoopRemoveSource(
                    CFRunLoopGetMain(),
                    source.as_concrete_TypeRef(),
                    *self.run_loop_mode as CFStringRef,
                )
            };
        }
    }

    fn next_instant(&self) -> Instant {
        if !self.callbacks.is_empty() {
            Instant::now()
        } else {
            let min = self.timers.values().map(|x| x.scheduled).min();
            min.unwrap_or_else(|| Instant::now() + Duration::from_secs(60 * 60))
        }
    }

    fn schedule(&mut self, state: Arc<Mutex<State>>) {
        self.unschedule();

        if !self.callbacks.is_empty() {
            if self.source.is_none() {
                self.create_source(state);
            }
            unsafe {
                CFRunLoopSourceSignal(
                    self.source
                        .as_ref()
                        .expect("Failed to create source")
                        .as_concrete_TypeRef(),
                );
                CFRunLoopWakeUp(CFRunLoopGetMain());
            }
        } else {
            let mutex = Arc::as_ptr(&state);
            let next = self.next_instant();
            let pending = next.saturating_duration_since(Instant::now());
            let fire_date = unsafe { CFAbsoluteTimeGetCurrent() } + pending.as_secs_f64();

            let mut context = CFRunLoopTimerContext {
                version: 0,
                info: mutex as *mut c_void,
                retain: Some(Self::retain),
                release: Some(Self::release),
                copyDescription: None,
            };

            let timer =
                CFRunLoopTimer::new(fire_date, 0.0, 0, 0, Self::on_timer, &mut context as *mut _);
            self.timer = Some(timer.clone());
            unsafe {
                CFRunLoopAddTimer(
                    CFRunLoopGetMain(),
                    timer.as_concrete_TypeRef(),
                    kCFRunLoopCommonModes,
                );
                CFRunLoopAddTimer(
                    CFRunLoopGetMain(),
                    timer.as_concrete_TypeRef(),
                    *self.run_loop_mode as CFStringRef,
                );
                CFRunLoopWakeUp(CFRunLoopGetMain());
            };
        }
    }

    extern "C" fn retain(data: *const c_void) -> *const c_void {
        let state = data as *const Mutex<State>;
        unsafe { Arc::increment_strong_count(state) }
        data
    }

    extern "C" fn release(data: *const c_void) {
        let state = data as *const Mutex<State>;
        unsafe { Arc::decrement_strong_count(state) };
    }

    extern "C" fn on_timer(_timer: CFRunLoopTimerRef, data: *mut c_void) {
        let state = data as *const Mutex<State>;
        let state = unsafe { Arc::from_raw(state) };
        Self::poll(state.clone());
        let _ = ManuallyDrop::new(state);
    }

    extern "C" fn on_source(data: *const c_void) {
        let state = data as *const Mutex<State>;
        let state = unsafe { Arc::from_raw(state) };
        Self::poll(state.clone());
        let _ = ManuallyDrop::new(state);
    }

    fn poll(state: Arc<Mutex<State>>) {
        let execution = state.lock().unwrap().get_pending_execution();
        for c in execution.callbacks {
            c();
        }
        for t in execution.timers {
            (t.callback)();
        }
        if !state.lock().unwrap().timers.is_empty() {
            let state_clone = state.clone();
            state.lock().unwrap().schedule(state_clone);
        }
    }
}

pub struct PlatformRunLoop {
    next_handle: Cell<HandleType>,
    state: Arc<Mutex<State>>,
}

impl PlatformRunLoop {
    pub fn new() -> Self {
        Self {
            next_handle: Cell::new(INVALID_HANDLE + 1),
            state: Arc::new(Mutex::new(State::new())),
        }
    }

    fn next_handle(&self) -> HandleType {
        let r = self.next_handle.get();
        self.next_handle.replace(r + 1);
        r
    }

    pub fn unschedule(&self, handle: HandleType) {
        let state_clone = self.state.clone();
        let mut state = self.state.lock().unwrap();
        state.timers.remove(&handle);
        state.schedule(state_clone);
    }

    pub fn schedule<F>(&self, in_time: Duration, callback: F) -> HandleType
    where
        F: FnOnce() + 'static,
    {
        let handle = self.next_handle();

        let state_clone = self.state.clone();
        let mut state = self.state.lock().unwrap();

        state.timers.insert(
            handle,
            Timer {
                scheduled: Instant::now() + in_time,
                callback: Box::new(callback),
            },
        );

        state.schedule(state_clone);

        handle
    }

    #[cfg(target_os = "macos")]
    pub fn run(&self) {
        #[cfg(any(test, feature = "mock"))]
        {
            super::main_thread_hack::ensure_ns_app_thinks_it_is_main_thread();
        }
        unsafe {
            let app = NSApplication::sharedApplication(nil);
            NSApplication::activateIgnoringOtherApps_(app, YES);
            NSApplication::run(app);
        }
    }

    #[cfg(target_os = "macos")]
    pub fn stop(&self) {
        self.state.lock().unwrap().unschedule();
        self.state.lock().unwrap().destroy_source();

        unsafe {
            let app = NSApplication::sharedApplication(nil);
            app.stop_(nil);

            let dummy_event: id = msg_send![class!(NSEvent),
                otherEventWithType: NSEventType::NSApplicationDefined
                location: NSPoint::new(0.0, 0.0)
                modifierFlags: 0
                timestamp: 0
                windowNumber: 0
                context: nil
                subtype: 0
                data1: 0
                data2: 0
            ];

            // To stop event loop immediately, we need to post event.
            let () = msg_send![app, postEvent: dummy_event atStart: YES];
        }
    }

    pub fn poll_once(&self) {
        let mode = self.state.lock().unwrap().run_loop_mode.clone();
        unsafe { CFRunLoopRunInMode(*mode as CFStringRef, 1.0, 1) };
    }

    pub fn new_sender(&self) -> PlatformRunLoopSender {
        PlatformRunLoopSender {
            state: self.state.clone(),
        }
    }
}

#[derive(Clone)]
pub struct PlatformRunLoopSender {
    state: Arc<Mutex<State>>,
}

#[allow(unused_variables)]
impl PlatformRunLoopSender {
    pub fn send<F>(&self, callback: F)
    where
        F: FnOnce() + 'static + Send,
    {
        let state_clone = self.state.clone();
        let mut state = self.state.lock().unwrap();
        state.callbacks.push(Box::new(callback));
        state.schedule(state_clone);
    }
}
