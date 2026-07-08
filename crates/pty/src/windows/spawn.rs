//! Spawn a child process attached to a pseudoconsole via `CreateProcessW`.
//!
//! The `HPCON` is passed through a process-thread attribute list
//! (`PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE`) with `EXTENDED_STARTUPINFO_PRESENT`;
//! handle inheritance is off, since the console handles travel via the
//! attribute rather than the handle table.

use std::ffi::{c_void, OsStr};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::process::ExitStatus;
use std::ptr::null_mut;

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Console::HPCON;
use windows::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
    InitializeProcThreadAttributeList, TerminateProcess, UpdateProcThreadAttribute,
    WaitForSingleObject, CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT, INFINITE,
    LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
    STARTUPINFOEXW,
};

use crate::spawn::SpawnOptions;

/// A spawned child: its process and main-thread handles plus its pid.
pub struct Child {
    process: OwnedHandle,
    pid: u32,
    // Kept only to close on drop; the main thread handle is otherwise unused.
    _thread: OwnedHandle,
}

impl Child {
    /// OS pid of the child.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Force-terminate the child.
    pub fn kill(&mut self) -> io::Result<()> {
        unsafe { TerminateProcess(self.handle(), 1) }.map_err(io::Error::other)
    }

    /// Block until the child exits, returning its exit status.
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        unsafe { WaitForSingleObject(self.handle(), INFINITE) };
        self.exit_status()
    }

    /// Non-blocking exit check; `None` while the child is still running.
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if unsafe { WaitForSingleObject(self.handle(), 0) } == WAIT_OBJECT_0 {
            Ok(Some(self.exit_status()?))
        } else {
            Ok(None)
        }
    }

    fn handle(&self) -> HANDLE {
        HANDLE(self.process.as_raw_handle())
    }

    fn exit_status(&self) -> io::Result<ExitStatus> {
        use std::os::windows::process::ExitStatusExt;
        let mut code = 0u32;
        unsafe { GetExitCodeProcess(self.handle(), &mut code) }.map_err(io::Error::other)?;
        Ok(ExitStatus::from_raw(code))
    }
}

/// Spawn `opts.argv` attached to the pseudoconsole `pcon`.
pub fn spawn_child(opts: &SpawnOptions, pcon: HPCON) -> io::Result<Child> {
    let mut cmdline = wide(&command_line(&opts.argv));
    let cwd = opts.cwd.as_ref().map(|p| wide(p.as_os_str()));
    let mut env = env_block(&opts.env);

    // Attribute list holding just the pseudoconsole. The first call reports the
    // required buffer size via an (ignored) failure.
    let mut size = 0usize;
    unsafe {
        let _ = InitializeProcThreadAttributeList(
            LPPROC_THREAD_ATTRIBUTE_LIST(null_mut()),
            1,
            0,
            &mut size,
        );
    }
    let mut buf = vec![0u8; size];
    let attrs = LPPROC_THREAD_ATTRIBUTE_LIST(buf.as_mut_ptr().cast());
    unsafe { InitializeProcThreadAttributeList(attrs, 1, 0, &mut size) }
        .map_err(io::Error::other)?;
    unsafe {
        UpdateProcThreadAttribute(
            attrs,
            0,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
            Some(pcon.0 as *const c_void),
            std::mem::size_of::<HPCON>(),
            None,
            None,
        )
    }
    .map_err(io::Error::other)?;

    let mut si = STARTUPINFOEXW::default();
    si.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    si.lpAttributeList = attrs;

    let mut pi = PROCESS_INFORMATION::default();

    let cwd_ptr = cwd
        .as_ref()
        .map(|w| PCWSTR(w.as_ptr()))
        .unwrap_or(PCWSTR::null());

    let result = unsafe {
        CreateProcessW(
            PCWSTR::null(),
            PWSTR(cmdline.as_mut_ptr()),
            None,
            None,
            false,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
            Some(env.as_mut_ptr().cast()),
            cwd_ptr,
            &si.StartupInfo,
            &mut pi,
        )
    };

    unsafe { DeleteProcThreadAttributeList(attrs) };
    result.map_err(io::Error::other)?;

    Ok(Child {
        process: unsafe { OwnedHandle::from_raw_handle(pi.hProcess.0) },
        _thread: unsafe { OwnedHandle::from_raw_handle(pi.hThread.0) },
        pid: pi.dwProcessId,
    })
}

/// A UTF-16, NUL-terminated copy of `s`, ready for a `PWSTR`/`PCWSTR`.
fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

/// Join argv into a single command line, quoting arguments that need it per the
/// `CommandLineToArgvW` rules.
fn command_line(argv: &[String]) -> std::ffi::OsString {
    let mut line = String::new();
    for (i, arg) in argv.iter().enumerate() {
        if i > 0 {
            line.push(' ');
        }
        line.push_str(&quote(arg));
    }
    line.into()
}

/// Quote a single argument for the Windows command-line convention.
fn quote(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '\n', '\u{b}', '"']) {
        return arg.to_string();
    }
    let mut out = String::from("\"");
    let mut backslashes = 0usize;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                // Escape the run of backslashes preceding the quote, then the quote.
                out.push_str(&"\\".repeat(backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.push_str(&"\\".repeat(backslashes));
                backslashes = 0;
                out.push(c);
            }
        }
    }
    out.push_str(&"\\".repeat(backslashes * 2));
    out.push('"');
    out
}

/// Build a `CREATE_UNICODE_ENVIRONMENT` block: the parent environment merged
/// with `overrides`, as `KEY=VALUE\0…\0\0` in UTF-16.
fn env_block(overrides: &[(String, String)]) -> Vec<u16> {
    use std::collections::BTreeMap;

    let mut vars: BTreeMap<String, String> = std::env::vars().collect();
    for (k, v) in overrides {
        vars.insert(k.clone(), v.clone());
    }

    let mut block = Vec::new();
    for (k, v) in &vars {
        block.extend(format!("{k}={v}").encode_utf16());
        block.push(0);
    }
    block.push(0);
    block
}
