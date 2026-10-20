//! The memory the program frees goes back to the system.
//!
//! The render threads of each document draw and free large pictures all the
//! time. On Linux the system allocator (glibc) keeps what a thread frees in
//! an area of its own and gives little of it back. Once the render threads
//! of a document have drawn and have nothing more to do (or a document
//! closes), and stay so for a second, a thread of its own asks the
//! allocator to give the free memory back (`malloc_trim`): never on the
//! thread of the interface, and not while the pages keep being drawn (a
//! scroll postpones it). The allocator also keeps at most four such areas
//! for all the threads, so that what one thread frees serves the others.
//! Elsewhere (Windows, macOS, other C libraries) the allocator of the system
//! works as in 0.2.0.

/// Start giving freed memory back (call once at start, before the windows).
pub fn set_up() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    imp::set_up();
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
mod imp {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    /// When the render threads last went idle (milliseconds since the start,
    /// 0: nothing to give back).
    static ASKED: AtomicU64 = AtomicU64::new(0);
    static START: OnceLock<Instant> = OnceLock::new();

    /// How long the render threads stay idle before the memory is given back.
    const QUIET: u64 = 1000;

    /// The areas of memory of the allocator for all the threads (its own
    /// limit is eight a processor).
    const ARENAS: i32 = 4;

    fn now() -> u64 {
        START.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
    }

    fn ask() {
        ASKED.store(now(), Ordering::Relaxed);
    }

    pub(super) fn set_up() {
        // SAFETY: called at the start of main, before any other thread.
        unsafe {
            libc::mallopt(libc::M_ARENA_MAX, ARENAS);
        }
        let _ = START.get_or_init(Instant::now);
        folitecta_core::render_service::on_idle(ask);
        let _ = std::thread::Builder::new()
            .name("folitecta-memory".into())
            .spawn(|| {
                loop {
                    std::thread::sleep(Duration::from_millis(250));
                    let asked = ASKED.load(Ordering::Relaxed);
                    if asked == 0 || now() < asked + QUIET {
                        continue;
                    }
                    if ASKED
                        .compare_exchange(asked, 0, Ordering::Relaxed, Ordering::Relaxed)
                        .is_err()
                    {
                        continue;
                    }
                    let started = Instant::now();
                    // SAFETY: malloc_trim only gives free memory of the
                    // allocator back to the system; it is safe at any time.
                    unsafe {
                        libc::malloc_trim(0);
                    }
                    crate::perf::TRIMS.bump();
                    crate::perf::TRIM_US.add(started.elapsed().as_micros() as u64);
                }
            });
    }
}
