use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicIsize, Ordering},
        Mutex, MutexGuard,
    },
};

use once_cell::sync::OnceCell;

use crate::{
    ffi::DartWeakPersistentHandle, util::Capsule, Context, GetMessageChannel, IsolateId,
    RUN_LOOP_SENDER,
};

///
/// FinalizableHandle can be used as payload in [`super::Value::FinalizableHandle`].
/// Will be received in Dart as instance of `FinalizableHandle`. When the Dart
/// instance gets garbage collected, the `finalizer` closure specified in
/// [`FinalizableHandle::new] will be invoked.
///
/// FinalizableHandle must be created on main thread, but other methods are thread safe.
///
#[derive(Debug, PartialEq, Eq, PartialOrd, Hash)]
pub struct FinalizableHandle {
    pub(super) id: isize,
}

impl FinalizableHandle {
    /// Creates a new finalizable handle instance. Must be created on main thread
    /// and the finalizer will also be invoked on main thread.
    ///
    /// If FinalizableHandle gets dropped the finalizer will not be executed.
    /// The finalizer is guaranteed to be executed even if the target isolate gets
    /// destroyed before it had chance to create dart weak persistent handle.
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
    pub fn new<F: FnOnce() + 'static>(
        external_size: isize,
        isolate_id: IsolateId,
        finalizer: F,
    ) -> Self {
        let id = next_handle();
        let mut state = FinalizableHandleState::get();
        state.objects.insert(
            id,
            FinalizableObjectState {
                handle: None,
                isolate_id,
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
        let state = FinalizableHandleState::get();
        state
            .objects
            .get(&self.id)
            .map(|s| s.handle.is_some())
            .unwrap_or(false)
    }

    /// Whether the Dart object was already garbage collected finalized.
    pub fn is_finalized(&self) -> bool {
        let state = FinalizableHandleState::get();
        !state.objects.contains_key(&self.id)
    }

    /// Updates the external size. This is a hint to Dart garbage collector.
    pub fn update_size(&self, size: isize) {
        let mut state = FinalizableHandleState::get();
        let object = state.objects.get_mut(&self.id);
        if let Some(object) = object {
            object.external_size = size;
            if object.handle.is_some() {
                let handle = self.id;
                let isolate_id = object.isolate_id;
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

    #[cfg(feature = "mock")]
    /// Allows simulating object finalizers
    pub fn finalize(&self) {
        let mut state = FinalizableHandleState::get();
        let mut object = state.objects.remove(&self.id);
        if let Some(mut object) = object.take() {
            if let Some(mut finalizer) = object.finalizer.take() {
                let sender = RUN_LOOP_SENDER
                    .get()
                    .expect("MessageChannel was not initialized!");
                sender.send(move || {
                    let finalizer = finalizer.take().unwrap();
                    finalizer();
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
        let mut state = FinalizableHandleState::get();
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
        // which can only be called from dart isolate.
        if !has_handle {
            state.objects.remove(&self.id);
        }
    }
}

pub(crate) struct FinalizableHandleState {
    objects: HashMap<isize, FinalizableObjectState>,
}

impl FinalizableHandleState {
    fn new() -> Self {
        Self {
            objects: HashMap::new(),
        }
    }

    pub(crate) fn get() -> MutexGuard<'static, Self> {
        static FUNCTIONS: OnceCell<Mutex<FinalizableHandleState>> = OnceCell::new();
        let state = FUNCTIONS.get_or_init(|| Mutex::new(FinalizableHandleState::new()));
        state.lock().unwrap()
    }

    /// Executes all finalizers that were not registered with the isolates.
    pub(crate) fn finalize_all(&mut self, isolate: IsolateId) {
        // TODO(knopp) use drain_filter once stable
        let to_remove: Vec<_> = self
            .objects
            .iter()
            .filter_map(|(id, object)| {
                if object.isolate_id == isolate && object.handle.is_none() {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();

        let finalizers: Vec<_> = to_remove
            .iter()
            .filter_map(|id| self.objects.remove(id))
            .filter_map(|mut f| f.finalizer.take())
            .collect();

        if !finalizers.is_empty() {
            RUN_LOOP_SENDER
                .get()
                .expect("MessageChannel was not initialized!")
                .send(move || {
                    for mut f in finalizers {
                        f.take().unwrap()();
                    }
                });
        }
    }
}

// We can't use Capsule for WeakPersistentHandle because it might be accessed
// from GC thread.
struct Movable<T>(T);

unsafe impl<T> Send for Movable<T> {}

struct FinalizableObjectState {
    handle: Option<Movable<DartWeakPersistentHandle>>,
    isolate_id: IsolateId,
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

#[cfg(not(feature = "mock"))]
pub(crate) mod finalizable_handle_native {
    use std::ffi::c_void;

    use crate::{
        ffi::{DartFunctions, DartHandle},
        IsolateId, RUN_LOOP_SENDER,
    };

    use super::{FinalizableHandleState, Movable};

    fn finalize_handle(handle: isize) {
        let object_state = {
            let mut state = FinalizableHandleState::get();
            state.objects.remove(&handle)
        };
        if let Some(mut object_state) = object_state {
            let finalizer = object_state.finalizer.take();
            // Finalizer may have been removed in FinalizableHandle::drop
            if let Some(mut finalizer) = finalizer {
                let finalizer = finalizer.take().unwrap();
                finalizer();
            }
        }
    }

    unsafe extern "C" fn finalizer(_isolate_callback_data: *mut c_void, peer: *mut c_void) {
        let handle = peer as isize;
        let mut state = FinalizableHandleState::get();
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
        let mut state = FinalizableHandleState::get();
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
            assert_eq!(object.isolate_id, isolate_id);
            return handle;
        }
        null_handle
    }

    pub(crate) unsafe extern "C" fn update_persistent_handle_size(id: isize) {
        let mut state = FinalizableHandleState::get();
        let object = state.objects.get_mut(&id);
        if let Some(object) = object {
            if let Some(handle) = object.handle.as_mut() {
                (DartFunctions::get().update_external_size)(handle.0, object.external_size);
            }
        }
    }
}

fn next_handle() -> isize {
    static mut COUNTER: AtomicIsize = AtomicIsize::new(0);
    unsafe { COUNTER.fetch_add(1, Ordering::SeqCst) }
}
