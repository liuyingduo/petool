#[cfg(target_os = "windows")]
use std::mem::transmute;

#[cfg(target_os = "windows")]
use windows::core::{IUnknown, Interface, BSTR, GUID, PCWSTR, VARIANT};
#[cfg(target_os = "windows")]
use windows::Win32::System::Com::{
    CoInitializeEx, CoUninitialize, IDispatch, COINIT_APARTMENTTHREADED, DISPATCH_METHOD,
    DISPATCH_PROPERTYGET, DISPATCH_PROPERTYPUT, DISPPARAMS,
};
#[cfg(target_os = "windows")]
use windows::Win32::System::Ole::{GetActiveObject, DISPID_PROPERTYPUT};

#[cfg(target_os = "windows")]
struct ComApartment;

#[cfg(target_os = "windows")]
impl ComApartment {
    fn init() -> Result<Self, String> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)
                .ok()
                .map_err(|e| format!("COM init failed: {}", e))?;
        }
        Ok(Self)
    }
}

#[cfg(target_os = "windows")]
impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

#[cfg(target_os = "windows")]
pub(super) fn with_active_application<T>(
    prog_id: &str,
    action: impl FnOnce(&IDispatch) -> Result<T, String>,
) -> Result<T, String> {
    let _apartment = ComApartment::init()?;
    let app = get_active_dispatch(prog_id)?;
    action(&app)
}

#[cfg(target_os = "windows")]
fn to_wide(value: &str) -> Vec<u16> {
    let mut wide: Vec<u16> = value.encode_utf16().collect();
    wide.push(0);
    wide
}

#[cfg(target_os = "windows")]
fn get_active_dispatch(prog_id: &str) -> Result<IDispatch, String> {
    let wide = to_wide(prog_id);
    let clsid = unsafe { windows::Win32::System::Com::CLSIDFromProgID(PCWSTR(wide.as_ptr())) }
        .map_err(|e| format!("CLSIDFromProgID('{}') failed: {}", prog_id, e))?;

    let mut unknown: Option<IUnknown> = None;
    unsafe { GetActiveObject(&clsid, None, &mut unknown) }
        .map_err(|e| format!("No active '{}' instance: {}", prog_id, e))?;

    let unknown = unknown.ok_or_else(|| format!("No active '{}' instance", prog_id))?;
    unknown
        .cast::<IDispatch>()
        .map_err(|e| format!("Active '{}' does not support IDispatch: {}", prog_id, e))
}

#[cfg(target_os = "windows")]
fn dispid(dispatch: &IDispatch, name: &str) -> Result<i32, String> {
    let wide = to_wide(name);
    let mut first = PCWSTR(wide.as_ptr());
    let mut id = 0i32;
    unsafe {
        dispatch
            .GetIDsOfNames(
                std::ptr::null::<GUID>(),
                &mut first as *mut PCWSTR,
                1,
                0,
                &mut id,
            )
            .map_err(|e| format!("GetIDsOfNames('{}') failed: {}", name, e))?;
    }
    Ok(id)
}

#[cfg(target_os = "windows")]
pub(super) fn get_property(dispatch: &IDispatch, name: &str) -> Result<VARIANT, String> {
    let property_id = dispid(dispatch, name)?;
    let params = DISPPARAMS::default();
    let mut result = VARIANT::default();
    unsafe {
        dispatch
            .Invoke(
                property_id,
                std::ptr::null(),
                0,
                DISPATCH_PROPERTYGET,
                &params,
                Some(&mut result),
                None,
                None,
            )
            .map_err(|e| format!("Property get '{}' failed: {}", name, e))?;
    }
    Ok(result)
}

#[cfg(target_os = "windows")]
pub(super) fn set_property(
    dispatch: &IDispatch,
    name: &str,
    mut value: VARIANT,
) -> Result<(), String> {
    let property_id = dispid(dispatch, name)?;
    let mut named = [DISPID_PROPERTYPUT];
    let params = DISPPARAMS {
        rgvarg: &mut value,
        rgdispidNamedArgs: named.as_mut_ptr(),
        cArgs: 1,
        cNamedArgs: 1,
    };
    unsafe {
        dispatch
            .Invoke(
                property_id,
                std::ptr::null(),
                0,
                DISPATCH_PROPERTYPUT,
                &params,
                None,
                None,
                None,
            )
            .map_err(|e| format!("Property set '{}' failed: {}", name, e))?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub(super) fn call_method(
    dispatch: &IDispatch,
    name: &str,
    mut args: Vec<VARIANT>,
) -> Result<VARIANT, String> {
    args.reverse();
    let method_id = dispid(dispatch, name)?;
    let params = DISPPARAMS {
        rgvarg: if args.is_empty() {
            std::ptr::null_mut()
        } else {
            args.as_mut_ptr()
        },
        rgdispidNamedArgs: std::ptr::null_mut(),
        cArgs: args.len() as u32,
        cNamedArgs: 0,
    };
    let mut result = VARIANT::default();
    unsafe {
        dispatch
            .Invoke(
                method_id,
                std::ptr::null(),
                0,
                DISPATCH_METHOD,
                &params,
                Some(&mut result),
                None,
                None,
            )
            .map_err(|e| format!("Method '{}' failed: {}", name, e))?;
    }
    Ok(result)
}

#[cfg(target_os = "windows")]
pub(super) fn variant_to_dispatch(value: &VARIANT) -> Result<IDispatch, String> {
    let raw = value.as_raw();
    let vt = unsafe { raw.Anonymous.Anonymous.vt };
    if vt != 9u16 {
        return Err(format!("Expected VT_DISPATCH, got vt={}", vt));
    }

    let ptr = unsafe { raw.Anonymous.Anonymous.Anonymous.pdispVal };
    if ptr.is_null() {
        return Err("COM returned null dispatch pointer".to_string());
    }

    let dispatch_ref: &IDispatch = unsafe { transmute(&ptr) };
    Ok(dispatch_ref.clone())
}

#[cfg(target_os = "windows")]
pub(super) fn variant_to_string(value: &VARIANT) -> Result<String, String> {
    BSTR::try_from(value)
        .map(|b| b.to_string())
        .map_err(|e| format!("Expected string result: {}", e))
}

#[cfg(target_os = "windows")]
pub(super) fn variant_to_i32(value: &VARIANT) -> Result<i32, String> {
    i32::try_from(value).map_err(|e| format!("Expected integer result: {}", e))
}

#[cfg(target_os = "windows")]
pub(super) fn variant_from_dispatch(value: &IDispatch) -> Result<VARIANT, String> {
    let unknown: IUnknown = value
        .cast()
        .map_err(|e| format!("Failed to cast dispatch to unknown: {}", e))?;
    Ok(VARIANT::from(unknown))
}
