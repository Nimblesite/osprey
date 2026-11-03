//! The dispatcher: a main-thread queue woken through the activity's ALooper, a
//! fixed worker pool for background tasks, and one timer thread for delays.
//!
//! gpui's own `ThreadedDispatcher` is test-support only, so this is the
//! production shape: `dispatch_on_main_thread` queues and wakes the event loop
//! (so a posted task never waits out the poll timeout), `dispatch_after` fires
//! on a timer thread and lands on the main queue like Apple's dispatch does.

use android_activity::{AndroidApp, AndroidAppWaker};
use gpui::{PlatformDispatcher, Priority, RunnableVariant};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

/// The worker pool size floor, mirroring gpui's own threaded dispatcher.
const MIN_THREADS: usize = 4;

type Task = RunnableVariant;

/// The Android platform dispatcher.
pub(crate) struct AndroidDispatcher {
    main: Mutex<VecDeque<Task>>,
    waker: AndroidAppWaker,
    timers: Arc<Timers>,
    pool: Arc<Pool>,
    main_thread: thread::ThreadId,
}

/// The background pool's shared FIFO, served by the worker threads.
struct Pool {
    queue: Mutex<VecDeque<Task>>,
    ready: Condvar,
}

impl AndroidDispatcher {
    /// Creates the dispatcher on the thread that will run the event loop; that
    /// thread becomes the main thread [`PlatformDispatcher::is_main_thread`] reports.
    pub fn new(app: &AndroidApp) -> Arc<Self> {
        let pool = Arc::new(Pool {
            queue: Mutex::new(VecDeque::new()),
            ready: Condvar::new(),
        });
        for worker in 0..workers() {
            let pool = pool.clone();
            let spawned = thread::Builder::new()
                .name(format!("gpui-android-{worker}"))
                .spawn(move || loop {
                    let task = {
                        let mut queue = locked(&pool.queue);
                        loop {
                            if let Some(task) = queue.pop_front() {
                                break task;
                            }
                            queue = wait(&pool.ready, queue);
                        }
                    };
                    run_quarantined(move || {
                        let _ = task.run();
                    });
                });
            if let Err(problem) = spawned {
                log::error!("failed to start GPUI Android worker {worker}: {problem}");
            }
        }
        let timers = Arc::new(Timers::default());
        let dispatcher = Arc::new(Self {
            main: Mutex::new(VecDeque::new()),
            waker: app.create_waker(),
            timers: timers.clone(),
            pool,
            main_thread: thread::current().id(),
        });
        let waker = app.create_waker();
        let spawned = thread::Builder::new()
            .name("gpui-android-timers".to_owned())
            .spawn(move || timer_loop(timers, waker));
        // The timer thread is structural; if it could not start, delayed tasks
        // would silently never fire, so the failure is logged loudly instead.
        if let Err(problem) = spawned {
            log::error!("failed to start the GPUI Android timer thread: {problem}");
        }
        dispatcher
    }

    /// Drains queued main-thread tasks. Called on the main thread once per
    /// event-loop iteration, after [`AndroidApp::poll_events`] returns or is woken.
    /// Fired timers land on the main queue first, so a woken loop delivers them.
    pub fn run_until_idle(&self) {
        {
            let ready = self.timers.take_ready();
            let mut main = locked(&self.main);
            for task in ready {
                main.push_front(task);
            }
        }
        loop {
            let task = locked(&self.main).pop_front();
            match task {
                Some(task) => run_quarantined(move || {
                    let _ = task.run();
                }),
                None => break,
            }
        }
    }
}

/// The worker pool size: the machine's parallelism, with gpui's own floor.
fn workers() -> usize {
    thread::available_parallelism().map_or(MIN_THREADS, |count| count.get().max(MIN_THREADS))
}

impl PlatformDispatcher for AndroidDispatcher {
    fn is_main_thread(&self) -> bool {
        thread::current().id() == self.main_thread
    }

    fn dispatch(&self, runnable: Task, _priority: Priority) {
        // Priority ordering can arrive with a priority queue; the pool is FIFO,
        // like gpui's Linux dispatcher's unsorted fallback.
        locked(&self.pool.queue).push_back(runnable);
        self.pool.ready.notify_one();
    }

    fn dispatch_on_main_thread(&self, runnable: Task, _priority: Priority) {
        locked(&self.main).push_back(runnable);
        self.waker.wake();
    }

    fn dispatch_after(&self, duration: Duration, runnable: Task) {
        self.timers.push(Instant::now() + duration, runnable);
    }

    fn spawn_realtime(&self, f: Box<dyn FnOnce() + Send>) {
        let spawned = thread::Builder::new()
            .name("gpui-android-realtime".to_owned())
            .spawn(move || run_quarantined(f));
        if let Err(problem) = spawned {
            log::error!("failed to start a GPUI Android realtime task: {problem}");
        }
    }
}

/// One timer due at a time, ordered by deadline then insertion.
struct Timer {
    due: Instant,
    order: u64,
    task: Task,
}

impl PartialEq for Timer {
    fn eq(&self, other: &Self) -> bool {
        self.due == other.due && self.order == other.order
    }
}

impl Eq for Timer {}

impl PartialOrd for Timer {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Timer {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.due, self.order).cmp(&(other.due, other.order))
    }
}

/// Timers waiting to fire, plus those already fired and parked for the main queue.
#[derive(Default)]
struct Timers {
    waiting: Mutex<BinaryHeap<Reverse<Timer>>>,
    ready: Mutex<VecDeque<Task>>,
    armed: Condvar,
    sequence: AtomicUsize,
}

impl Timers {
    /// Arms one timer.
    fn push(&self, due: Instant, task: Task) {
        let order = self.sequence.fetch_add(1, Ordering::Relaxed) as u64;
        locked(&self.waiting).push(Reverse(Timer { due, order, task }));
        self.armed.notify_one();
    }

    /// Moves every due timer onto the ready queue.
    fn fire_due(&self, now: Instant) -> bool {
        let mut fired = false;
        let mut waiting = locked(&self.waiting);
        while waiting
            .peek()
            .is_some_and(|Reverse(timer)| timer.due <= now)
        {
            if let Some(Reverse(timer)) = waiting.pop() {
                locked(&self.ready).push_back(timer.task);
                fired = true;
            }
        }
        fired
    }

    /// Takes every fired timer.
    fn take_ready(&self) -> VecDeque<Task> {
        std::mem::take(&mut *locked(&self.ready))
    }
}

/// The timer thread: waits for the nearest deadline, fires due timers onto the
/// ready queue and wakes the main loop so they are delivered without delay.
fn timer_loop(timers: Arc<Timers>, waker: AndroidAppWaker) {
    let mut waiting = locked(&timers.waiting);
    loop {
        let Some(deadline) = waiting.peek().map(|Reverse(timer)| timer.due) else {
            waiting = timers
                .armed
                .wait(waiting)
                .unwrap_or_else(PoisonError::into_inner);
            continue;
        };
        let now = Instant::now();
        if deadline > now {
            let (guard, _) = timers
                .armed
                .wait_timeout(waiting, deadline - now)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            waiting = guard;
        } else {
            drop(waiting);
            timers.fire_due(Instant::now());
            waker.wake();
            waiting = locked(&timers.waiting);
        }
    }
}

/// Locks without panicking on a poisoned mutex.
fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Waits on a condvar with a queue guard, poison-tolerant.
fn wait<'a, T>(condvar: &Condvar, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
    condvar.wait(guard).unwrap_or_else(PoisonError::into_inner)
}

/// Runs one task, quarantining its panic into the log: the event loop must
/// keep serving events after a task panicked.
fn run_quarantined(task: impl FnOnce() + Send) {
    if panic::catch_unwind(AssertUnwindSafe(task)).is_err() {
        log::error!("a dispatched task panicked; the event loop continues");
    }
}
