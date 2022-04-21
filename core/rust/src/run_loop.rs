use std::{
    cell::{RefCell, UnsafeCell},
    future::Future,
    marker::PhantomData,
    rc::Rc,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::Poll,
    time::Duration,
};

use futures::{
    future::LocalBoxFuture,
    task::{waker_ref, ArcWake},
    FutureExt,
};

use crate::util::BlockingVariable;

use super::{
    platform::run_loop::{PlatformRunLoop, PlatformRunLoopSender},
    Handle,
};

pub struct RunLoop {
    pub platform_run_loop: Rc<PlatformRunLoop>,
}

impl RunLoop {
    pub fn new() -> Self {
        Self {
            platform_run_loop: Rc::new(PlatformRunLoop::new()),
        }
    }

    #[must_use]
    pub fn schedule<F>(&self, in_time: Duration, callback: F) -> Handle
    where
        F: FnOnce() + 'static,
    {
        let run_loop = self.platform_run_loop.clone();
        let handle = run_loop.schedule(in_time, callback);
        Handle::new(move || {
            run_loop.unschedule(handle);
        })
    }

    // Convenience method to schedule callback on next run loop turn
    #[must_use]
    pub fn schedule_next<F>(&self, callback: F) -> Handle
    where
        F: FnOnce() + 'static,
    {
        self.schedule(Duration::from_secs(0), callback)
    }

    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
    pub fn run(&self) {
        self.platform_run_loop.run()
    }

    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
    pub fn stop(&self) {
        self.platform_run_loop.stop()
    }

    pub fn new_sender(&self) -> RunLoopSender {
        RunLoopSender {
            thread_id: get_thread_id(),
            platform_sender: self.platform_run_loop.new_sender(),
        }
    }

    // Spawn the future with current run loop being the executor;
    pub fn spawn<T: 'static>(&self, future: impl Future<Output = T> + 'static) -> JoinHandle<T> {
        let future = future.boxed_local();
        let task = Arc::new(Task {
            sender: self.new_sender(),
            future: UnsafeCell::new(future),
            value: RefCell::new(None),
            waker: RefCell::new(None),
        });
        ArcWake::wake_by_ref(&task);
        JoinHandle {
            task,
            _data: PhantomData {},
        }
    }
}

// Can be used to send callbacks from other threads to be executed on run loop thread
#[derive(Clone)]
pub struct RunLoopSender {
    thread_id: usize,
    platform_sender: PlatformRunLoopSender,
}

impl RunLoopSender {
    /// Schedules the callback to be executed on run loop and returns immediately.
    pub fn send<F>(&self, callback: F)
    where
        F: FnOnce() + 'static + Send,
    {
        self.platform_sender.send(callback)
    }

    /// Schedules the callback on run loop and blocks until it is invoked.
    /// If current thread is run loop thread the callback will be invoked immediately
    /// (otherwise it would deadlock).
    pub fn send_and_wait<F, R>(&self, callback: F) -> R
    where
        F: FnOnce() -> R + 'static + Send,
        R: Send + 'static,
    {
        if get_thread_id() == self.thread_id {
            callback()
        } else {
            let var = BlockingVariable::<R>::new();
            let var_clone = var.clone();
            self.send(move || {
                var_clone.set(callback());
            });
            var.get_blocking()
        }
    }
}

fn get_thread_id() -> usize {
    thread_local!(static THREAD_ID: usize = next_thread_id());
    THREAD_ID.with(|&x| x)
}

fn next_thread_id() -> usize {
    static mut COUNTER: AtomicUsize = AtomicUsize::new(0);
    unsafe { COUNTER.fetch_add(1, Ordering::SeqCst) }
}

//
//
//

struct Task<T> {
    sender: RunLoopSender,
    future: UnsafeCell<LocalBoxFuture<'static, T>>,
    value: RefCell<Option<T>>,
    waker: RefCell<Option<std::task::Waker>>,
}

// Tasks can only be spawned on run loop thread and will only be executed
// on run loop thread. ArcWake however doesn't know this.
unsafe impl<T> Send for Task<T> {}
unsafe impl<T> Sync for Task<T> {}

impl<T: 'static> Task<T> {
    fn poll(self: &std::sync::Arc<Self>) -> Poll<T> {
        let waker = waker_ref(self).clone();
        let context = &mut core::task::Context::from_waker(&waker);
        unsafe {
            let future = &mut *self.future.get();
            future.as_mut().poll(context)
        }
    }
}

impl<T: 'static> ArcWake for Task<T> {
    fn wake_by_ref(arc_self: &std::sync::Arc<Self>) {
        let arc_self = arc_self.clone();
        let sender = arc_self.sender.clone();
        sender.send(move || {
            if arc_self.value.borrow().is_none() {
                if let Poll::Ready(value) = arc_self.poll() {
                    *arc_self.value.borrow_mut() = Some(value);
                }
            }
            if arc_self.value.borrow().is_some() {
                if let Some(waker) = arc_self.waker.borrow_mut().take() {
                    waker.wake();
                }
            }
        });
    }
}

pub struct JoinHandle<T> {
    task: Arc<Task<T>>,
    // Task has unsafe `Send` and `Sync`, but that is only because we know
    // it will not be polled from another thread. This is to ensure that
    // JoinHandle is neither Send nor Sync.
    _data: PhantomData<*const ()>,
}

impl<T: 'static> Future for JoinHandle<T> {
    type Output = T;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let value = self.task.value.borrow_mut().take();
        match value {
            Some(value) => Poll::Ready(value),
            None => {
                self.task
                    .waker
                    .borrow_mut()
                    .get_or_insert_with(|| cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

// IMPORTANT
// Test must be run with  cargo test -- --test-threads=1
// otherwise they will likely crash
#[cfg(test)]
mod tests {
    use crate::{
        util::{Capsule, FutureCompleter},
        RunLoop,
    };
    use std::{
        cell::RefCell,
        rc::Rc,
        sync::{Arc, Mutex},
        thread,
        time::{Duration, Instant},
    };

    #[test]
    fn test_run() {
        let rl = Rc::new(RunLoop::new());
        let rlc = rl.clone();
        let next_called = Rc::new(RefCell::new(false));
        let next_called_clone = next_called.clone();
        let start = Instant::now();
        rl.schedule(Duration::from_millis(50), move || {
            next_called_clone.replace(true);
            rlc.stop();
        })
        .detach();
        assert_eq!(*next_called.borrow(), false);
        rl.run();
        assert_eq!(*next_called.borrow(), true);
        assert!(start.elapsed() >= Duration::from_millis(50));
    }

    #[test]
    fn test_sender() {
        let run_loop = Rc::new(RunLoop::new());
        let rl = Arc::new(Mutex::new(Capsule::new(run_loop.clone())));
        let sender = run_loop.new_sender();
        let stop_called = Arc::new(Mutex::new(false));
        let stop_called_clone = stop_called.clone();
        // make sure to spawn the thread when run loop is already running
        run_loop
            .schedule_next(|| {
                thread::spawn(move || {
                    sender.send(move || {
                        let rl = rl.lock().unwrap();
                        let rl = rl.get_ref().unwrap();
                        *stop_called_clone.lock().unwrap() = true;
                        rl.stop();
                    });
                });
            })
            .detach();
        assert_eq!(*stop_called.lock().unwrap(), false);
        run_loop.run();
        assert_eq!(*stop_called.lock().unwrap(), true);
    }

    async fn wait(run_loop: Rc<RunLoop>, duration: Duration) {
        let (future, completer) = FutureCompleter::<()>::new();
        run_loop
            .schedule(duration, move || {
                completer.complete(());
            })
            .detach();
        future.await
    }

    #[test]
    fn test_sync() {
        let run_loop = Rc::new(RunLoop::new());
        let w = wait(run_loop.clone(), Duration::from_millis(50));
        let run_loop_clone = run_loop.clone();
        run_loop.spawn(async move {
            w.await;
            run_loop_clone.stop();
        });
        let start = Instant::now();
        run_loop.run();
        assert!(start.elapsed() >= Duration::from_millis(50));
    }
}
