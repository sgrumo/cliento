use std::panic;

use clientmail_domain::ports::Blocking;
use tokio::runtime::{Builder, Handle, Runtime};
use tokio::task::{JoinError, JoinHandle};

use crate::CoreError;

/// Workers drive async network work, which waits most of the time; two keep the app light.
const WORKER_THREADS: usize = 2;
/// `SQLite` is one connection behind a mutex, so more blocking threads would only queue on it.
/// Idle blocking threads exit after Tokio's 10 second keep-alive.
const MAX_BLOCKING_THREADS: usize = 8;
const THREAD_NAME: &str = "clientmail-core";

/// The Tokio runtime the core runs on. One per [`crate::CoreClient`] (decision record 0008).
#[derive(Debug)]
pub(crate) struct CoreRuntime {
    handle: Handle,
    /// `None` only inside `Drop`, which takes the runtime out to shut it down.
    runtime: Option<Runtime>,
}

impl CoreRuntime {
    /// Starts the runtime's threads.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Runtime`] when Tokio cannot set up a driver.
    ///
    /// # Panics
    ///
    /// Tokio panics, instead of returning an error, when the operating system refuses to start a
    /// thread.
    pub(crate) fn new() -> Result<Self, CoreError> {
        Self::with_threads(WORKER_THREADS, MAX_BLOCKING_THREADS)
    }

    fn with_threads(workers: usize, max_blocking: usize) -> Result<Self, CoreError> {
        let runtime = Builder::new_multi_thread()
            .worker_threads(workers)
            .max_blocking_threads(max_blocking)
            .thread_name(THREAD_NAME)
            .enable_all()
            .build()
            .map_err(|_| CoreError::Runtime)?;
        Ok(Self {
            handle: runtime.handle().clone(),
            runtime: Some(runtime),
        })
    }

    /// Runs `work` on the blocking pool, so it never holds a worker.
    ///
    /// The work starts at once; the returned future only waits for it, and any executor can poll
    /// it (Swift's, or a test's), with or without a Tokio context. Dropping the future does not
    /// stop the work.
    ///
    /// # Arguments
    ///
    /// * `work` - Blocking code: `SQLite` calls, platform callbacks. It must own what it uses
    ///   (`'static`), because it runs on another thread after this call returns.
    ///
    /// # Panics
    ///
    /// A panic inside `work` resumes in the caller, so `UniFFI` reports it as it does for a
    /// synchronous call.
    pub(crate) fn blocking<T, F>(&self, work: F) -> impl Future<Output = T> + Send + 'static
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        blocking_on(&self.handle, work)
    }

    /// Runs an async task on the workers. Network futures must run here: an exported method's
    /// body is polled by Swift, outside any Tokio context.
    ///
    /// Await the handle with [`finished`]; abort it through `JoinHandle::abort_handle`.
    ///
    /// # Arguments
    ///
    /// * `task` - The future to run. It must own what it uses (`'static`).
    pub(crate) fn spawn<F>(&self, task: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.handle.spawn(task)
    }

    /// Returns the [`Blocking`] port for engine use cases, usable from spawned tasks.
    pub(crate) fn blocking_port(&self) -> RuntimeBlocking {
        RuntimeBlocking {
            handle: self.handle.clone(),
        }
    }
}

/// The domain's [`Blocking`] port over the runtime's blocking pool (decision record 0009).
///
/// It holds a `Handle`, not the runtime, so engine use cases can keep it inside spawned tasks.
#[derive(Debug, Clone)]
pub(crate) struct RuntimeBlocking {
    handle: Handle,
}

impl Blocking for RuntimeBlocking {
    fn run<T, F>(&self, work: F) -> impl Future<Output = T> + Send + 'static
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        blocking_on(&self.handle, work)
    }
}

fn blocking_on<T, F>(handle: &Handle, work: F) -> impl Future<Output = T> + Send + 'static
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let task = handle.spawn_blocking(work);
    async move { finished(task.await).unwrap_or_else(|| cancelled()) }
}

/// Unwraps a task's result: its value, or `None` when it was aborted or the runtime shut down.
///
/// # Panics
///
/// A panic inside the task resumes here, so `UniFFI` reports it as it does for a synchronous call.
pub(crate) fn finished<T>(result: Result<T, JoinError>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(error) => match error.try_into_panic() {
            Ok(payload) => panic::resume_unwind(payload),
            Err(_cancelled) => None,
        },
    }
}

/// A blocking task is only cancelled when the runtime shuts down before it starts. The future from
/// [`CoreRuntime::blocking`] does not keep the runtime alive, so this stays unreachable only while
/// the runtime lives until the future is awaited: every `CoreClient` method keeps it alive,
/// because `UniFFI` keeps the client alive until the call returns, and a sign-in session holds
/// the runtime too. A spawned task that outlives both would hit this when the runtime shuts down,
/// which only happens when nothing is waiting for the task any more.
#[expect(
    clippy::panic,
    reason = "unreachable while the runtime outlives the futures that wait on it"
)]
fn cancelled() -> ! {
    panic!("the core runtime shut down while a call was waiting")
}

impl Drop for CoreRuntime {
    fn drop(&mut self) {
        // A plain drop waits for running tasks and panics inside async code (for example a test
        // that drops the client in a Tokio task); `shutdown_background` does neither.
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    const WAIT: Duration = Duration::from_secs(5);

    #[test]
    fn blocking_returns_the_value_without_a_tokio_context() {
        let runtime = CoreRuntime::new().unwrap();

        let value = pollster::block_on(runtime.blocking(|| 42));

        assert_eq!(value, 42, "value from the blocking pool");
    }

    #[test]
    fn blocking_work_runs_on_core_threads() {
        let runtime = CoreRuntime::new().unwrap();

        let name = pollster::block_on(
            runtime.blocking(|| std::thread::current().name().map(ToOwned::to_owned)),
        );

        assert_eq!(name.as_deref(), Some(THREAD_NAME), "core thread");
    }

    #[test]
    fn parked_blocking_work_does_not_hold_the_only_worker() {
        let runtime = CoreRuntime::with_threads(1, 2).unwrap();
        let (release, gate) = mpsc::channel::<()>();
        let (done, finished) = mpsc::channel();

        let parked = runtime.blocking(move || gate.recv_timeout(WAIT));
        runtime.handle.spawn(async move { done.send(()) });
        let worker_ran = finished.recv_timeout(WAIT);
        release.send(()).unwrap();
        let unparked = pollster::block_on(parked);

        assert!(
            worker_ran.is_ok(),
            "the worker ran while blocking work was parked"
        );
        assert!(unparked.is_ok(), "the parked work was released");
    }

    #[test]
    fn a_panic_in_blocking_work_resumes_in_the_caller() {
        let runtime = CoreRuntime::new().unwrap();

        let result = panic::catch_unwind(|| {
            pollster::block_on(runtime.blocking(|| panic!("boom")));
        });

        let payload = result.expect_err("panic resumed");
        assert_eq!(
            payload.downcast_ref::<&str>(),
            Some(&"boom"),
            "the work's own panic, not the cancellation one"
        );
    }

    #[test]
    fn spawned_tasks_run_on_core_threads_and_return_their_value() {
        let runtime = CoreRuntime::new().unwrap();

        let name = pollster::block_on(
            runtime.spawn(async { std::thread::current().name().map(ToOwned::to_owned) }),
        );

        assert_eq!(
            finished(name).flatten().as_deref(),
            Some(THREAD_NAME),
            "worker thread"
        );
    }

    #[test]
    fn an_aborted_task_finishes_with_none() {
        let runtime = CoreRuntime::new().unwrap();
        let task = runtime.spawn(std::future::pending::<()>());

        task.abort();
        let result = finished(pollster::block_on(task));

        assert_eq!(result, None, "aborted");
    }

    #[test]
    fn the_blocking_port_works_inside_a_spawned_task() {
        let runtime = CoreRuntime::new().unwrap();
        let port = runtime.blocking_port();

        let value = pollster::block_on(runtime.spawn(async move { port.run(|| 42).await }));

        assert_eq!(finished(value), Some(42), "value through the port");
    }

    #[test]
    fn dropping_inside_a_tokio_task_does_not_panic() {
        let runtime = CoreRuntime::new().unwrap();
        let outer = Builder::new_current_thread().build().unwrap();

        outer.block_on(async move { drop(runtime) });
    }
}
