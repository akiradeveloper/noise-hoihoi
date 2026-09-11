//! Audited boundary to our pinned C shim, not to unstable IREE structs.
#![allow(unsafe_code)]

use crate::Device;
use anyhow::{Context, Result, bail, ensure};
use libloading::Library;
use std::{
    ffi::{CString, c_char, c_int, c_void},
    path::PathBuf,
    ptr::NonNull,
    sync::OnceLock,
};

type Callback = unsafe extern "C" fn(*mut c_void, *const u8, usize, *const u8, usize, c_int);
type Devices = unsafe extern "C" fn(Callback, *mut c_void, *mut u8, usize) -> c_int;
type Create = unsafe extern "C" fn(
    *const c_char,
    *const u8,
    usize,
    *const f32,
    *mut *mut c_void,
    *mut u8,
    usize,
) -> c_int;
type Destroy = unsafe extern "C" fn(*mut c_void);
type Reset = unsafe extern "C" fn(*mut c_void, *const f32, *mut u8, usize) -> c_int;
type Process = unsafe extern "C" fn(*mut c_void, *const f32, *mut f32, *mut u8, usize) -> c_int;
// The bytecode module borrows its backing bytes. Keep them aligned and static.
#[repr(C, align(64))]
struct Aligned<const N: usize>([u8; N]);
static GPU_MODEL: Aligned<{ include_bytes!("../model/dpdfnet8.vmfb").len() }> =
    Aligned(*include_bytes!("../model/dpdfnet8.vmfb"));

static CPU_MODEL: Aligned<{ include_bytes!("../model/dpdfnet8-cpu.vmfb").len() }> =
    Aligned(*include_bytes!("../model/dpdfnet8-cpu.vmfb"));

static CPU_AVX2_MODEL: Aligned<{ include_bytes!("../model/dpdfnet8-cpu-avx2.vmfb").len() }> =
    Aligned(*include_bytes!("../model/dpdfnet8-cpu-avx2.vmfb"));

fn cpu_model() -> &'static [u8] {
    #[cfg(target_arch = "x86_64")]
    if std::is_x86_feature_detected!("avx2") && std::is_x86_feature_detected!("fma") {
        return &CPU_AVX2_MODEL.0;
    }
    &CPU_MODEL.0
}

struct Api {
    devices: Devices,
    create: Create,
    destroy: Destroy,
    reset: Reset,
    process: Process,
    _library: Library,
}
static API: OnceLock<Result<Api, String>> = OnceLock::new();
fn api() -> Result<&'static Api> {
    API.get_or_init(|| load().map_err(|e| format!("{e:#}")))
        .as_ref()
        .map_err(|e| anyhow::anyhow!(e.clone()))
}
fn load() -> Result<Api> {
    let path = if let Some(path) = std::env::var_os("NOISE_IREE_LIBRARY") {
        PathBuf::from(path)
    } else {
        let exe = std::env::current_exe()?;
        exe.parent()
            .context("executable has no parent")?
            .join("iree")
            .join(if cfg!(windows) {
                "noise_iree.dll"
            } else {
                "libnoise_iree.so"
            })
    };
    // SAFETY: the application ships this pinned shim. Function signatures match
    // native/noise_iree.c, version is checked, and the library stays loaded forever.
    unsafe {
        let library = Library::new(&path)
            .with_context(|| format!("Inference library unavailable: {}", path.display()))?;
        let version = library.get::<unsafe extern "C" fn() -> u32>(b"noise_iree_abi\0")?;
        ensure!(version() == 1, "Inference library ABI mismatch");
        Ok(Api {
            devices: *library.get(b"noise_iree_devices\0")?,
            create: *library.get(b"noise_iree_create\0")?,
            destroy: *library.get(b"noise_iree_destroy\0")?,
            reset: *library.get(b"noise_iree_reset\0")?,
            process: *library.get(b"noise_iree_process\0")?,
            _library: library,
        })
    }
}
fn check(code: c_int, error: &[u8]) -> Result<()> {
    if code != 0 {
        let end = error.iter().position(|v| *v == 0).unwrap_or(error.len());
        bail!("{}", String::from_utf8_lossy(&error[..end]));
    }
    Ok(())
}
unsafe extern "C" fn device_callback(
    user: *mut c_void,
    path: *const u8,
    path_len: usize,
    name: *const u8,
    name_len: usize,
    kind: c_int,
) {
    // SAFETY: called synchronously by devices with the borrowed vector and valid
    // string spans; no references escape the callback and it performs no fallible parsing.
    unsafe {
        let devices = &mut *user.cast::<Vec<Device>>();
        devices.push(Device {
            uri: format!(
                "vulkan://{}",
                String::from_utf8_lossy(std::slice::from_raw_parts(path, path_len))
            ),
            name: String::from_utf8_lossy(std::slice::from_raw_parts(name, name_len)).into_owned(),
            integrated: kind == 1,
        });
    }
}
pub fn devices() -> Result<Vec<Device>> {
    let api = api()?;
    let mut devices = Vec::<Device>::new();
    let mut error = [0_u8; 4096];
    // SAFETY: callback's vector and error storage remain valid for this synchronous call.
    let code = unsafe {
        (api.devices)(
            device_callback,
            std::ptr::from_mut(&mut devices).cast(),
            error.as_mut_ptr(),
            error.len(),
        )
    };
    check(code, &error)?;
    Ok(devices)
}

pub struct Stream {
    pointer: NonNull<c_void>,
    api: &'static Api,
}
// SAFETY: this is an exclusively owned session. All calls require &mut self;
// IREE permits moving a session between threads when calls never overlap.
unsafe impl Send for Stream {}
impl Stream {
    pub fn new(uri: &str, state: &[f32; 90_228]) -> Result<Self> {
        let api = api()?;
        let model: &'static [u8] = if uri == "cpu" {
            cpu_model()
        } else {
            &GPU_MODEL.0
        };
        let uri = CString::new(uri)?;
        let mut pointer = std::ptr::null_mut();
        let mut error = [0_u8; 4096];
        // SAFETY: all spans have the exact sizes required by the shim, model lifetime
        // is static, and ownership of the returned handle transfers to this Stream.
        let code = unsafe {
            (api.create)(
                uri.as_ptr(),
                model.as_ptr(),
                model.len(),
                state.as_ptr(),
                &raw mut pointer,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        check(code, &error)?;
        Ok(Self {
            pointer: NonNull::new(pointer).context("Inference returned an empty session")?,
            api,
        })
    }
    pub fn reset(&mut self, state: &[f32; 90_228]) -> Result<()> {
        let mut error = [0_u8; 4096];
        // SAFETY: exclusively owned live session and correctly sized input/output spans.
        let code = unsafe {
            (self.api.reset)(
                self.pointer.as_ptr(),
                state.as_ptr(),
                error.as_mut_ptr(),
                error.len(),
            )
        };
        check(code, &error)
    }
    pub fn process(&mut self, input: &[f32; 962], output: &mut [f32; 962]) -> Result<()> {
        let mut error = [0_u8; 4096];
        // SAFETY: exclusively owned live session, disjoint correctly sized spans,
        // and synchronous call completing readback before returning.
        let code = unsafe {
            (self.api.process)(
                self.pointer.as_ptr(),
                input.as_ptr(),
                output.as_mut_ptr(),
                error.as_mut_ptr(),
                error.len(),
            )
        };
        check(code, &error)
    }
}
impl Drop for Stream {
    fn drop(&mut self) {
        // SAFETY: this is the unique owning handle; destruction happens exactly once.
        unsafe {
            (self.api.destroy)(self.pointer.as_ptr());
        }
    }
}
