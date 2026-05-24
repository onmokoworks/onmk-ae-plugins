use crate::engine::EngineError;
use std::path::PathBuf;
use std::sync::OnceLock;

static ORT_READY: OnceLock<Result<(), String>> = OnceLock::new();

#[cfg(target_os = "macos")]
const ORT_NOT_FOUND_MSG: &str =
    "ONNX Runtime dylib not found (expected Contents/Frameworks/libonnxruntime.1.24.4.dylib)";

#[cfg(windows)]
const ORT_NOT_FOUND_MSG: &str =
    "ONNX Runtime DLL not found (expected onnxruntime.dll next to DepthONNX.aex)";

/// Directory containing the loaded plug-in binary (.aex on Windows, bundle MacOS dir on macOS).
pub fn plugin_install_dir() -> Option<PathBuf> {
    plugin_module_path().and_then(|p| p.parent().map(PathBuf::from))
}

pub fn ensure_ort() -> Result<(), EngineError> {
    ORT_READY
        .get_or_init(|| init_ort().map_err(|e| e.to_string()))
        .clone()
        .map_err(EngineError::SessionInit)
}

fn init_ort() -> Result<(), EngineError> {
    let path = resolve_ort_dylib().ok_or_else(|| {
        EngineError::SessionInit(ORT_NOT_FOUND_MSG.into())
    })?;

    ort::init_from(&path)
        .map_err(|e| EngineError::SessionInit(format!("init_from failed: {e}")))?
        .with_name("DepthONNX")
        .commit();

    Ok(())
}

fn resolve_ort_dylib() -> Option<PathBuf> {
    if let Some(path) = ort_dylib_beside_plugin() {
        if path.is_file() {
            return Some(path);
        }
    }

    if let Ok(path) = std::env::var("ORT_DYLIB_PATH") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }

    if let Some(path) = option_env!("ORT_DYLIB_PATH") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }

    None
}

fn ort_dylib_beside_plugin() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        plugin_install_dir().map(|dir| dir.join("onnxruntime.dll"))
    }

    #[cfg(target_os = "macos")]
    {
        let macos_dir = plugin_install_dir()?;
        let candidate = macos_dir.join("../Frameworks/libonnxruntime.1.24.4.dylib");
        candidate.canonicalize().ok().or(Some(candidate))
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        None
    }
}

fn plugin_module_path() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        use std::ffi::CStr;
        use std::path::Path;

        extern "C" {
            fn dladdr(addr: *const std::ffi::c_void, info: *mut DlInfo) -> i32;
        }

        #[repr(C)]
        struct DlInfo {
            dli_fname: *const i8,
            dli_fbase: *mut std::ffi::c_void,
            dli_sname: *const i8,
            dli_saddr: *mut std::ffi::c_void,
        }

        extern "C" fn plugin_anchor() {}

        let mut info = DlInfo {
            dli_fname: std::ptr::null(),
            dli_fbase: std::ptr::null_mut(),
            dli_sname: std::ptr::null(),
            dli_saddr: std::ptr::null_mut(),
        };

        let ok = unsafe {
            dladdr(
                plugin_anchor as extern "C" fn() as *const std::ffi::c_void,
                &mut info,
            )
        };
        if ok == 0 || info.dli_fname.is_null() {
            return None;
        }

        let plugin_path = unsafe { CStr::from_ptr(info.dli_fname) }.to_str().ok()?;
        Some(Path::new(plugin_path).to_path_buf())
    }

    #[cfg(windows)]
    {
        use std::ffi::c_void;
        use std::os::windows::ffi::OsStringExt;
        use windows_sys::Win32::System::LibraryLoader::{
            GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
            GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        };

        extern "C" fn plugin_anchor() {}

        let mut module = std::mem::MaybeUninit::<*mut c_void>::uninit();
        let ok = unsafe {
            GetModuleHandleExW(
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS
                    | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                plugin_anchor as *const extern "C" fn() as *const u16,
                module.as_mut_ptr(),
            )
        };
        if ok == 0 {
            return None;
        }

        let mut buf = vec![0u16; 1024];
        let len =
            unsafe { GetModuleFileNameW(module.assume_init(), buf.as_mut_ptr(), buf.len() as u32) };
        if len == 0 || len as usize >= buf.len() {
            return None;
        }
        buf.truncate(len as usize);
        Some(PathBuf::from(std::ffi::OsString::from_wide(&buf)))
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    {
        None
    }
}
