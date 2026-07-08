//! Cross-platform process control for the daemon manager: liveness checks and
//! termination by pid. Unix uses signals; Windows uses the process APIs — there
//! is no SIGTERM for a detached console daemon, so `terminate` is a hard
//! `TerminateProcess` there, same as `kill`.

/// True if a process with this pid exists.
#[cfg(unix)]
pub fn alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

/// Ask the process to stop (SIGTERM).
#[cfg(unix)]
pub fn terminate(pid: u32) {
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
}

/// Force-kill the process (SIGKILL).
#[cfg(unix)]
pub fn kill(pid: u32) {
    unsafe {
        libc::kill(pid as i32, libc::SIGKILL);
    }
}

#[cfg(windows)]
use windows::Win32::Foundation::CloseHandle;
#[cfg(windows)]
use windows::Win32::System::Threading::{
    OpenProcess, TerminateProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
};

/// True if a handle can be opened for this pid (the process exists).
#[cfg(windows)]
pub fn alive(pid: u32) -> bool {
    unsafe {
        match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) => {
                let _ = CloseHandle(h);
                true
            }
            Err(_) => false,
        }
    }
}

/// Terminate the process. No SIGTERM equivalent for a detached console daemon,
/// so this hard-terminates (same as [`kill`]).
#[cfg(windows)]
pub fn terminate(pid: u32) {
    kill(pid);
}

/// Force-kill the process.
#[cfg(windows)]
pub fn kill(pid: u32) {
    unsafe {
        if let Ok(h) = OpenProcess(PROCESS_TERMINATE, false, pid) {
            let _ = TerminateProcess(h, 1);
            let _ = CloseHandle(h);
        }
    }
}
