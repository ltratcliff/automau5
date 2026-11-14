//! Keeps the system awake via each OS's power-management API.
//!
//! * Windows: `SetThreadExecutionState` (per-thread, hence [`AwakeGuard`] is `!Send`).
//!   Debug with `powercfg /requests`.
//! * macOS: IOKit power assertions; allowed in the App Sandbox. Debug with
//!   `pmset -g assertions`.
//! * Linux and other Unix: a `systemd-inhibit` idle lock held by a child
//!   process. Whether the screen still blanks depends on the desktop
//!   environment, so `keep_display_on` is best-effort there. Debug with
//!   `systemd-inhibit --list`.

use crate::Error;
use std::marker::PhantomData;

#[cfg(not(windows))]
const REASON: &str = "Automau5 keep-awake mode is on";

/// Keeps the system (and optionally the display) awake until dropped.
///
/// Create and drop it on the same thread, and keep that thread alive while
/// it's held: Windows ties the request to the calling thread.
pub struct AwakeGuard {
    _imp: imp::Imp,
    // Opt out of Send/Sync to enforce the same-thread rule at compile time.
    _not_send: PhantomData<*const ()>,
}

impl AwakeGuard {
    pub fn new(keep_display_on: bool) -> Result<Self, Error> {
        Ok(Self {
            _imp: imp::Imp::new(keep_display_on)?,
            _not_send: PhantomData,
        })
    }
}

#[cfg(windows)]
mod imp {
    use super::Error;
    use windows_sys::Win32::System::Power::{
        SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED,
    };

    pub struct Imp;

    impl Imp {
        pub fn new(keep_display_on: bool) -> Result<Self, Error> {
            let mut flags = ES_CONTINUOUS | ES_SYSTEM_REQUIRED;
            if keep_display_on {
                flags |= ES_DISPLAY_REQUIRED;
            }
            // SAFETY: plain Win32 call with valid flags; returns 0 on failure.
            if unsafe { SetThreadExecutionState(flags) } == 0 {
                return Err(Error::new(format!(
                    "could not keep the system awake: {}",
                    std::io::Error::last_os_error()
                )));
            }
            Ok(Imp)
        }
    }

    impl Drop for Imp {
        fn drop(&mut self) {
            // SAFETY: as above. ES_CONTINUOUS alone clears this thread's request.
            unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{Error, REASON};
    use objc2_core_foundation::CFString;
    use objc2_io_kit::{
        kIOPMAssertionLevelOn, kIOReturnSuccess, IOPMAssertionCreateWithName, IOPMAssertionID,
        IOPMAssertionRelease,
    };

    pub struct Imp {
        assertions: Vec<IOPMAssertionID>,
    }

    impl Imp {
        pub fn new(keep_display_on: bool) -> Result<Self, Error> {
            let mut imp = Imp { assertions: Vec::new() };
            imp.assert("PreventUserIdleSystemSleep")?;
            if keep_display_on {
                imp.assert("PreventUserIdleDisplaySleep")?;
            }
            Ok(imp)
        }

        // On error, Drop releases whatever was already created.
        fn assert(&mut self, kind: &'static str) -> Result<(), Error> {
            let kind = CFString::from_static_str(kind);
            let name = CFString::from_static_str(REASON);
            let mut id: IOPMAssertionID = 0;
            // SAFETY: valid CFStrings and a valid out-pointer.
            let result = unsafe {
                IOPMAssertionCreateWithName(Some(&kind), kIOPMAssertionLevelOn, Some(&name), &mut id)
            };
            if result != kIOReturnSuccess {
                return Err(Error::new(format!(
                    "could not keep the system awake: IOKit error {result:#010x}"
                )));
            }
            self.assertions.push(id);
            Ok(())
        }
    }

    impl Drop for Imp {
        fn drop(&mut self) {
            for &id in &self.assertions {
                IOPMAssertionRelease(id);
            }
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use super::{Error, REASON};
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    pub struct Imp {
        child: Child,
    }

    impl Imp {
        pub fn new(_keep_display_on: bool) -> Result<Self, Error> {
            // The lock lasts as long as `systemd-inhibit` runs, and it runs as
            // long as `cat` does. `cat` exits when its stdin (our pipe) closes,
            // so the lock is released even if this process is killed.
            let mut child = Command::new("systemd-inhibit")
                .args(["--what=idle", "--who=Automau5", "--mode=block"])
                .arg(format!("--why={REASON}"))
                .arg("cat")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| Error::new(format!("could not run systemd-inhibit: {e}")))?;

            // It fails fast when logind is unavailable or refuses the lock;
            // otherwise it keeps running.
            let deadline = Instant::now() + Duration::from_millis(300);
            while Instant::now() < deadline {
                if let Ok(Some(status)) = child.try_wait() {
                    return Err(Error::new(format!(
                        "could not keep the system awake: systemd-inhibit exited ({status})"
                    )));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(Imp { child })
        }
    }

    impl Drop for Imp {
        fn drop(&mut self) {
            drop(self.child.stdin.take());
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
