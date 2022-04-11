use std::{
    collections::HashMap,
    ffi::c_void,
    sync::{
        atomic::{AtomicIsize, Ordering},
        Mutex, MutexGuard,
    },
};

use once_cell::sync::OnceCell;

use crate::{
    ffi::{DartFunctions, DartHandle, DartWeakPersistentHandle},
    util::Capsule,
    Context, GetMessageChannel, IsolateId, RUN_LOOP_SENDER,
};

///
/// FinalizableHandle can be used as payload in [`super::Value::FinalizableHandle`].
/// Will be received in Dart as instance of `FinalizableHandle`. When the Dart
/// instance gets garbage collected, the `finalizer` closure specified in
///  [`FinalizableHandle::new] will be invoked.
///
/// FinalizableHandle must be created on main thread, but other methods are thread safe.
///
#[derive(Debug, PartialEq, PartialOrd, Hash)]
pub struct FinalizableHandle {
    pub(super) id: isize,
}

impl FinalizableHandle {
    /// Creates a new finalizable handle instance. Must be created on main thread
    /// and the finalizer will also be invoked on main thread.
    ///
    /// # Arguments
    ///
    /// * `finalizer` - closure that will be executed on main thread when the
    ///                 Dart object associated with this handle is garbage collected.
    ///                 The closure will not be invoked when this `FinalizableHandle`
    ///                 is dropped.
    ///
    /// * `external_size` - hit to garbage collector about how much memory is taken by
    ///                     native object. Used when determining memory pressure.
    ///
    pub fn new<F: FnOnce() + 'static>(external_size: isize, finalizer: F) -> Self {
        let id = next_handle();
        let mut state = State::get();
        state.objects.insert(
            id,
            FinalizableObjectState {
                handle: None,
                isolate_id: None,
                external_size,
                finalizer: Some(Capsule::new_with_sender(
                    Box::new(finalizer),
                    Context::get().run_loop().new_sender(),
                )),
            },
        );
        Self { id }
    }

    /// Whether this handle is attached to a Dart object. This will be `false`
    /// initially and becomes `true` once the Finalizable handle is send to Dart.
    /// `false` after the Dart counterpart gets garbage collected.
    pub fn is_attached(&self) -> bool {
        let state = State::get();
        state
            .objects
            .get(&self.id)
            .map(|s| s.handle.is_some())
            .unwrap_or(false)
    }

    /// Whether the Dart object was already garbage collected finalized.
    pub fn is_finalized(&self) -> bool {
        let state = State::get();
        state.objects.contains_key(&self.id)
    }

    /// Updates the external size. This is a hint to Dart garbage collector.
    pub fn update_size(&self, size: isize) {
        let mut state = State::get();
        let object = state.objects.get_mut(&self.id);
        if let Some(object) = object {
            object.external_size = size;
            if let Some(isolate_id) = object.isolate_id {
                let handle = self.id;
                // The actual dart method to update isolate size must be called from
                // Dart thread, so we ask message channel to relay the request,
                // which should result in a call to 'update_persistent_handle_size'.
                RUN_LOOP_SENDER
                    .get()
                    .expect("MessageChannel was not initialized!")
                    .send(move || {
                        Context::get()
                            .message_channel()
                            .request_update_external_size(isolate_id, handle);
                    });
            }
        }
    }
}

//
//
//

impl Drop for FinalizableHandle {
    fn drop(&mut self) {
        let mut state = State::get();
        let object = state.objects.get_mut(&self.id);
        let mut has_handle = true;
        if let Some(object) = object {
            // Capsule was created with run loop sender and will properly schedule drop
            // on main thread.
            object.finalizer.take();
            has_handle = object.handle.is_some();
        }
        // This finalizable handle has never been sent to dart, we can safely remove
        // it from objects map. If it was sent from dart we'll only remove it from
        // dart finalizer because we need to call delete_weak_persistent_handle on it
        // which can only be called from dart id.
        if !has_handle {
            state.objects.remove(&self.id);
        }
    }
}

struct State {
    objects: HashMap<isize, FinalizableObjectState>,
}

impl State {
    fn new() -> Self {
        Self {
            objects: HashMap::new(),
        }
    }

    fn get() -> MutexGuard<'static, Self> {
        static FUNCTIONS: OnceCell<Mutex<State>> = OnceCell::new();
        let state = FUNCTIONS.get_or_init(|| Mutex::new(State::new()));
        state.lock().unwrap()
    }
}

// We can't use Capsule for WeakPersistentHandle because it might be accessed
// from GC thread.
struct Movable<T>(T);

unsafe impl<T> Send for Movable<T> {}

struct FinalizableObjectState {
    handle: Option<Movable<DartWeakPersistentHandle>>,
    isolate_id: Option<IsolateId>,
    external_size: isize,
    finalizer: Option<Capsule<Box<dyn FnOnce()>>>,
}

impl Drop for FinalizableObjectState {
    fn drop(&mut self) {
        if self.handle.is_some() {
            // This should never happen. Dart finalizer should have been called first
            // to clean-up the handle
            panic!("FinalizableObjectState is being dropped with active handle");
        }
    }
}

fn finalize_handle(handle: isize) {
    let object_state = {
        let mut state = State::get();
        state.objects.remove(&handle)
    };
    if let Some(mut object_state) = object_state {
        let mut finalizer = object_state
            .finalizer
            .take()
            .expect("Finalizer executed more than once");
        let finalizer = finalizer.take().unwrap();
        finalizer();
    }
}

unsafe extern "C" fn finalizer(_isolate_callback_data: *mut c_void, peer: *mut c_void) {
    let handle = peer as isize;
    let mut state = State::get();
    let object = state.objects.get_mut(&handle);
    if let Some(object) = object {
        if let Some(handle) = object.handle.take() {
            (DartFunctions::get().delete_weak_persistent_handle)(handle.0);
        }
    }
    let sender = RUN_LOOP_SENDER
        .get()
        .expect("MessageChannel was not initialized!");
    sender.send(move || {
        finalize_handle(handle);
    });
}

pub(crate) unsafe extern "C" fn attach_weak_persistent_handle(
    handle: DartHandle,
    id: isize,
    null_handle: DartHandle,
    isolate_id: IsolateId,
) -> DartHandle {
    let mut state = State::get();
    let object = state.objects.get_mut(&id);
    if let Some(object) = object {
        if let Some(handle) = object.handle.as_mut() {
            let real_handle = (DartFunctions::get().handle_from_weak_persistent)(handle.0);
            // Try to return existing object if there is any
            if !real_handle.is_null() {
                return real_handle;
            }
        }
        let weak_handle = (DartFunctions::get().new_weak_persistent_handle)(
            handle,
            id as *mut c_void,
            object.external_size,
            finalizer,
        );
        object.handle = Some(Movable(weak_handle));
        object.isolate_id = Some(isolate_id);
        return handle;
    }
    null_handle
}

pub(crate) unsafe extern "C" fn update_persistent_handle_size(id: isize) {
    let mut state = State::get();
    let object = state.objects.get_mut(&id);
    if let Some(object) = object {
        if let Some(handle) = object.handle.as_mut() {
            (DartFunctions::get().update_external_size)(handle.0, object.external_size);
        }
    }
}

fn next_handle() -> isize {
    static mut COUNTER: AtomicIsize = AtomicIsize::new(0);
    unsafe { COUNTER.fetch_add(1, Ordering::SeqCst) }
}
