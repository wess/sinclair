//! libkrun, loaded at runtime. libkrun turns the calling process into the VM
//! (`krun_start_enter` never returns once the guest is running), so this only
//! ever runs in the dedicated boot process, never in the GUI.
//!
//! Loaded with `dlopen` rather than linked so a Sinclair build without the
//! libraries still starts, and simply reports OS Tabs as unavailable.

// Only the boot path is platform-specific; elsewhere its helpers sit unused.
#![cfg_attr(not(unix), allow(dead_code))]

use std::ffi::{c_char, CString};
#[cfg(unix)]
use std::ffi::c_void;
use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
const KRUN: &str = "libkrun.1.dylib";
#[cfg(target_os = "macos")]
const KRUNFW: &str = "libkrunfw.5.dylib";
#[cfg(not(target_os = "macos"))]
const KRUN: &str = "libkrun.so.1";
#[cfg(not(target_os = "macos"))]
const KRUNFW: &str = "libkrunfw.so.5";

/// Where the libraries ship relative to the executable: the app bundle's
/// `Frameworks`, a Linux package's private lib dir, or `krun/` beside a dev
/// build (`scripts/krun.sh` puts them there).
pub fn libdir(exe: &Path) -> Option<PathBuf> {
  let dir = exe.parent()?;
  [
    dir.join("../Frameworks"),
    dir.join("../lib/sinclair"),
    dir.join("krun"),
  ]
  .into_iter()
  .find(|d| d.join(KRUN).is_file() && d.join(KRUNFW).is_file())
}

/// What the guest runs, and on what.
pub struct Spec<'a> {
  pub rootfs: &'a Path,
  pub workdir: &'a str,
  pub exec: &'a str,
  pub argv: &'a [String],
  pub env: &'a [String],
  pub cpus: u8,
  pub memory_mib: u32,
}

/// Configure and enter the VM. Returns only on failure; on success the
/// process exits with the guest command's status when the VM shuts down.
#[cfg(unix)]
pub fn enter(libdir: &Path, spec: &Spec) -> String {
  match unsafe { boot(libdir, spec) } {
    Ok(()) => "the VM exited unexpectedly".into(),
    Err(e) => e,
  }
}

#[cfg(not(unix))]
pub fn enter(_: &Path, _: &Spec) -> String {
  "built-in OS Tabs need a macOS host".into()
}

#[cfg(unix)]
unsafe fn boot(libdir: &Path, spec: &Spec) -> Result<(), String> {
  // libkrun dlopens libkrunfw by bare name; loading it first, by full path
  // and globally, is what lets that lookup find ours.
  open(&libdir.join(KRUNFW), libc::RTLD_NOW | libc::RTLD_GLOBAL)?;
  let lib = open(&libdir.join(KRUN), libc::RTLD_NOW | libc::RTLD_LOCAL)?;

  let create_ctx: unsafe extern "C" fn() -> i32 = sym(lib, c"krun_create_ctx")?;
  let set_vm_config: unsafe extern "C" fn(u32, u8, u32) -> i32 = sym(lib, c"krun_set_vm_config")?;
  let set_root: unsafe extern "C" fn(u32, *const c_char) -> i32 = sym(lib, c"krun_set_root")?;
  let set_workdir: unsafe extern "C" fn(u32, *const c_char) -> i32 = sym(lib, c"krun_set_workdir")?;
  let set_exec: unsafe extern "C" fn(
    u32,
    *const c_char,
    *const *const c_char,
    *const *const c_char,
  ) -> i32 = sym(lib, c"krun_set_exec")?;
  let start_enter: unsafe extern "C" fn(u32) -> i32 = sym(lib, c"krun_start_enter")?;

  let ctx = create_ctx();
  if ctx < 0 {
    return Err(format!("krun_create_ctx failed ({ctx})"));
  }
  let ctx = ctx as u32;
  check("krun_set_vm_config", set_vm_config(ctx, spec.cpus, spec.memory_mib))?;

  let root = cstr(&spec.rootfs.to_string_lossy())?;
  check("krun_set_root", set_root(ctx, root.as_ptr()))?;
  let workdir = cstr(spec.workdir)?;
  check("krun_set_workdir", set_workdir(ctx, workdir.as_ptr()))?;

  let exec = cstr(spec.exec)?;
  let argv = cstrs(spec.argv)?;
  let env = cstrs(spec.env)?;
  let argv_ptrs = ptrs(&argv);
  let env_ptrs = ptrs(&env);
  check(
    "krun_set_exec",
    set_exec(ctx, exec.as_ptr(), argv_ptrs.as_ptr(), env_ptrs.as_ptr()),
  )?;

  // Reached only if the VM could not start. Without the hypervisor
  // entitlement Hypervisor.framework refuses and libkrun reports EINVAL.
  check("krun_start_enter", start_enter(ctx)).map_err(|e| {
    format!("{e} (is the executable signed with the com.apple.security.hypervisor entitlement?)")
  })
}

#[cfg(unix)]
unsafe fn open(path: &Path, flags: i32) -> Result<*mut c_void, String> {
  let c_path = cstr(&path.to_string_lossy())?;
  let handle = libc::dlopen(c_path.as_ptr(), flags);
  if handle.is_null() {
    let err = libc::dlerror();
    let why = if err.is_null() {
      String::from("unknown error")
    } else {
      std::ffi::CStr::from_ptr(err).to_string_lossy().into_owned()
    };
    return Err(format!("load {}: {why}", path.display()));
  }
  Ok(handle)
}

#[cfg(unix)]
unsafe fn sym<T: Copy>(lib: *mut c_void, name: &std::ffi::CStr) -> Result<T, String> {
  let ptr = libc::dlsym(lib, name.as_ptr());
  if ptr.is_null() {
    return Err(format!("libkrun has no {}", name.to_string_lossy()));
  }
  Ok(std::mem::transmute_copy(&ptr))
}

fn check(call: &str, rc: i32) -> Result<(), String> {
  if rc < 0 {
    Err(format!("{call} failed: {}", std::io::Error::from_raw_os_error(-rc)))
  } else {
    Ok(())
  }
}

fn cstr(s: &str) -> Result<CString, String> {
  CString::new(s).map_err(|_| format!("nul byte in `{s}`"))
}

fn cstrs(items: &[String]) -> Result<Vec<CString>, String> {
  items.iter().map(|s| cstr(s)).collect()
}

/// A null-terminated pointer array borrowing `items`.
fn ptrs(items: &[CString]) -> Vec<*const c_char> {
  items
    .iter()
    .map(|s| s.as_ptr())
    .chain(std::iter::once(std::ptr::null()))
    .collect()
}
