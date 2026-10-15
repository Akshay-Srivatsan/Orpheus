//! Thread priorities. Audio must never stutter, even if the interface slows
//! down: the output callbacks (via cpal's `realtime` feature) and the deck
//! threads that feed them run at real-time priority; analysis and stem
//! separation run at the lowest priority.
//!
//! On Linux, real-time scheduling is granted by RealtimeKit, which limits how
//! long a real-time thread may compute without blocking (RLIMIT_RTTIME,
//! usually 200 ms; exceeding the soft limit raises SIGXCPU, the hard limit
//! SIGKILL). Deck threads therefore block briefly every few milliseconds of
//! work (`RtThread::breathe`) and drop to normal priority for one-off work like
//! opening a song (`RtThread::normal`), and SIGXCPU is ignored.

use audio_thread_priority::{demote_current_thread_from_real_time, promote_current_thread_to_real_time, RtPriorityHandle};
use std::time::{Duration, Instant};

/// Continuous work after which a real-time deck thread blocks briefly.
const MAX_BURST: Duration = Duration::from_millis(5);
/// Period requested for real-time scheduling: a 10 ms slice.
const PERIOD_FRAMES_PER_SEC: u32 = 100;

enum Granted {
    /// Through the platform's audio API (RealtimeKit on Linux).
    Platform(RtPriorityHandle),
    /// Linux: SCHED_FIFO set directly (allowed when RLIMIT_RTPRIO permits, e.g. the audio group).
    #[cfg(target_os = "linux")]
    Fifo,
}

/// A thread promoted to real-time priority (if the system allows it).
pub struct RtThread {
    handle: Option<Granted>,
    rate: u32,
    since_block: Instant,
    /// The outcome has been logged (once; `normal` re-raises for every song).
    announced: bool,
}

impl RtThread {
    /// Promotes the current thread; falls back to normal priority with a log message.
    pub fn promote(rate: u32) -> Self {
        ignore_sigxcpu();
        let mut t = Self { handle: None, rate: rate.max(8000), since_block: Instant::now(), announced: false };
        t.raise();
        t
    }

    fn raise(&mut self) {
        if self.handle.is_some() {
            return;
        }
        let first = !self.announced;
        self.announced = true;
        self.handle = match promote_current_thread_to_real_time(self.rate / PERIOD_FRAMES_PER_SEC, self.rate) {
            Ok(h) => {
                if first {
                    log::info!("{}: real-time priority", thread_name());
                }
                Some(Granted::Platform(h))
            }
            Err(e) => fifo().inspect(|_| {
                if first {
                    log::info!("{}: real-time priority (SCHED_FIFO)", thread_name());
                }
            }).or_else(|| {
                log::warn!("{}: no real-time priority ({e}); audio may stutter under heavy load", thread_name());
                None
            }),
        };
        self.since_block = Instant::now();
    }

    /// Runs `f` at normal priority (for long, one-off work such as opening a song).
    pub fn normal<R>(&mut self, f: impl FnOnce() -> R) -> R {
        let was_rt = match self.handle.take() {
            Some(Granted::Platform(h)) => {
                if let Err(e) = demote_current_thread_from_real_time(h) {
                    log::warn!("{}: could not leave real-time priority: {e}", thread_name());
                }
                true
            }
            #[cfg(target_os = "linux")]
            Some(Granted::Fifo) => {
                set_policy(libc::SCHED_OTHER, 0);
                true
            }
            None => false,
        };
        let r = f();
        if was_rt {
            self.raise();
        }
        r
    }

    /// Call between chunks of work: blocks for a moment after `MAX_BURST` of continuous work.
    pub fn breathe(&mut self) {
        if self.since_block.elapsed() >= MAX_BURST {
            self.blocked(Duration::from_micros(50));
        }
    }

    /// Sleeps (a blocking call, which resets the real-time budget).
    pub fn blocked(&mut self, d: Duration) {
        std::thread::sleep(d);
        self.since_block = Instant::now();
    }

    /// Notes that the thread just blocked some other way (e.g. waiting on a channel).
    pub fn did_block(&mut self) {
        self.since_block = Instant::now();
    }
}

/// Lowest priority for background work (analysis, stem separation).
pub fn lower_current_thread() {
    if let Err(e) = thread_priority::set_current_thread_priority(thread_priority::ThreadPriority::Min) {
        log::debug!("{}: could not lower thread priority: {e:?}", thread_name());
    }
}

#[cfg(target_os = "linux")]
fn fifo() -> Option<Granted> {
    // A moderate real-time priority, below typical audio servers' (PipeWire uses 88).
    set_policy(libc::SCHED_FIFO, 20).then_some(Granted::Fifo)
}

/// Sets the calling thread's scheduling policy (Linux applies it per thread).
#[cfg(target_os = "linux")]
fn set_policy(policy: libc::c_int, priority: libc::c_int) -> bool {
    let param = libc::sched_param { sched_priority: priority };
    // SAFETY: pid 0 is the calling thread; `param` is a valid sched_param.
    unsafe { libc::sched_setscheduler(0, policy, &param) == 0 }
}

#[cfg(not(target_os = "linux"))]
fn fifo() -> Option<Granted> {
    None
}

fn thread_name() -> String {
    std::thread::current().name().unwrap_or("thread").to_string()
}

#[cfg(target_os = "linux")]
fn ignore_sigxcpu() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    // SAFETY: setting a signal's disposition to SIG_IGN has no preconditions.
    ONCE.call_once(|| unsafe {
        libc::signal(libc::SIGXCPU, libc::SIG_IGN);
    });
}

#[cfg(not(target_os = "linux"))]
fn ignore_sigxcpu() {}
