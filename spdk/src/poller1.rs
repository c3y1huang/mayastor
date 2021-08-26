use std::{
    ffi::{c_void, CString},
    fmt,
    ptr::NonNull,
    time::Duration,
};

use spdk_sys::{
    spdk_poller,
    spdk_poller_pause,
    spdk_poller_register,
    spdk_poller_register_named,
    spdk_poller_resume,
    spdk_poller_unregister,
};

/// Structure holding our function and context
struct PollCtx<'a> {
    poll_fn: Box<dyn FnMut() -> i32 + 'a>
}

/// TODO
extern "C" fn inner_poller_callback(ctx: *mut c_void) -> i32 {
    let poll = unsafe { &mut *(ctx as *mut PollCtx) };
    (poll.poll_fn)()
}

/// Poller structure that allows us to pause, stop, resume periodic tasks
pub struct Poller1<'a> {
    inner: NonNull<spdk_poller>,
    ctx: NonNull<PollCtx<'a>>,
    stopped: bool,
    name: String,
}

impl<'a> fmt::Debug for Poller1<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Poller")
            .field("name", &self.name)
            .field("stopped", &self.stopped)
            .finish()
    }
}

impl<'a> Poller1<'a> {
    /// stop the given poller and consumes self
    pub fn stop(mut self) {
        unsafe {
            spdk_poller_unregister(&mut self.inner.as_ptr());
            Box::from_raw(self.ctx.as_ptr());
            self.stopped = true;
        }
    }

    /// Pauses the poller.
    pub fn pause(&self) {
        unsafe {
            spdk_poller_pause(self.inner.as_ptr());
        }
    }

    /// Resumes the poller.
    pub fn resume(&self) {
        unsafe {
            spdk_poller_resume(self.inner.as_ptr());
        }
    }
}

impl<'a> Drop for Poller1<'a> {
    fn drop(&mut self) {
        if !self.stopped {
            unsafe {
                spdk_poller_unregister(&mut self.inner.as_ptr());
                Box::from_raw(self.ctx.as_ptr());
            }
        }
    }
}

/// Builder type to create a new poller.
pub struct PollerBuilder<'a> {
    name: Option<CString>,
    interval: std::time::Duration,
    poll_fn: Option<Box<dyn FnMut() -> i32 + 'a>>,
}

impl<'a> PollerBuilder<'a> {
    /// create a new nameless poller that runs every time the thread the poller
    /// is created on is polled
    pub fn new() -> Self {
        Self {
            name: None,
            interval: Duration::from_micros(0),
            poll_fn: None,
        }
    }

    /// create the poller with a given name
    pub fn with_name<S: Into<Vec<u8>>>(mut self, name: S) -> Self {
        self.name = Some(
            CString::new(name)
                .expect("poller name is invalid or out of memory"),
        );
        self
    }

    /// set the interval for the poller in usec
    pub fn with_interval(mut self, usec: u64) -> Self {
        self.interval = Duration::from_micros(usec);
        self
    }

    /// set the function for this poller
    pub fn with_poll_fn(mut self, poll_fn: impl FnMut() -> i32 + 'a) -> Self {
        self.poll_fn = Some(Box::new(poll_fn));
        self
    }

    /// build a  new poller object
    pub fn build(mut self) -> Poller1<'a> {
        let poll_fn = self
            .poll_fn
            .take()
            .expect("can not start poller without poll function");

        let ctx = NonNull::new(Box::into_raw(Box::new(PollCtx {
            poll_fn
        }))).expect("failed to allocate new poller context");

        let name;
        let inner = NonNull::new(unsafe {
            if self.name.is_none() {
                name = "<unnamed>".to_string();
                spdk_poller_register(
                    Some(inner_poller_callback),
                    ctx.as_ptr().cast(),
                    self.interval.as_micros() as u64,
                )
            } else {
                name =
                    String::from(self.name.as_ref().unwrap().to_str().unwrap());

                spdk_poller_register_named(
                    Some(inner_poller_callback),
                    ctx.as_ptr().cast(),
                    self.interval.as_micros() as u64,
                    self.name.as_ref().unwrap().as_ptr(),
                )
            }
        })
        .expect("failed to register poller");

        Poller1 {
            inner,
            ctx,
            stopped: false,
            name,
        }
    }
}
