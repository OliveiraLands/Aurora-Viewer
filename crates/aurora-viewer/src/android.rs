//! Android entry point (`android_main`), storage paths, and runtime permission gate.

use std::ffi::{CStr, CString, c_char, c_void};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use winit::platform::android::activity::AndroidApp;

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();
static ANDROID_APP: OnceLock<AndroidApp> = OnceLock::new();
static GAMEPAD_SEEN: AtomicBool = AtomicBool::new(false);
static GAMEPAD_AXES: [AtomicU32; 4] = [AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)];
static GAMEPAD_KEYS: AtomicU32 = AtomicU32::new(0);
static LOADING_HIDDEN: AtomicBool = AtomicBool::new(false);
static BACK_CONSUMED: AtomicBool = AtomicBool::new(false);
static BACK_PRESSED: AtomicBool = AtomicBool::new(false);
static SAFE_INSETS_PX: [AtomicU32; 4] = [AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)];
static IME_BOTTOM_PX: AtomicU32 = AtomicU32::new(0);
static SOFT_KEYBOARD_SHOWN: AtomicBool = AtomicBool::new(false);

/// Runtime (dangerous) permissions declared in `AndroidManifest.xml` that must
/// be checked and requested before activating the main viewer.
const RUNTIME_PERMISSIONS: &[&str] = &["android.permission.RECORD_AUDIO"];

const PERMISSION_REQUEST_CODE: i32 = 1001;
const JNI_VERSION_1_6: i32 = 0x0001_0006;

#[derive(Debug, Clone, Copy)]
enum GateState {
    Unchecked,
    Waiting { requested_at: Instant, saw_pause: bool },
    Ready,
}

static GATE_STATE: Mutex<GateState> = Mutex::new(GateState::Unchecked);

/// Base writable directory for config, cache, and logs on Android.
pub(crate) fn data_dir() -> PathBuf {
    DATA_DIR.get().cloned().unwrap_or_else(|| PathBuf::from("."))
}

/// Returns `true` while the viewer is waiting for the Android system permission
/// dialog before creating the main window and renderer.
pub(crate) fn waiting_for_permissions() -> bool {
    GATE_STATE.lock().map(|s| matches!(*s, GateState::Waiting { .. })).unwrap_or(false)
}

/// Records that the Activity paused while waiting for the system permission dialog.
pub(crate) fn notify_suspended() {
    if let Ok(mut state) = GATE_STATE.lock()
        && let GateState::Waiting { saw_pause, .. } = &mut *state
    {
        *saw_pause = true;
    }
}

/// Checks whether required Android runtime permissions are granted and, if not,
/// triggers the system permission dialog before the main viewer starts.
///
/// Returns `true` when the viewer may proceed with window and renderer creation.
pub(crate) fn ensure_permissions_before_start(from_resumed: bool) -> bool {
    let Ok(mut state) = GATE_STATE.lock() else {
        return true;
    };
    if matches!(*state, GateState::Ready) {
        return true;
    }
    let Some(app) = ANDROID_APP.get() else {
        *state = GateState::Ready;
        return true;
    };

    let missing: Vec<&'static str> = RUNTIME_PERMISSIONS
        .iter()
        .copied()
        .filter(|&perm| !check_self_permission(app, perm))
        .collect();

    if missing.is_empty() {
        log::info!("Android runtime permissions granted");
        *state = GateState::Ready;
        return true;
    }

    match *state {
        GateState::Unchecked => {
            log::info!("Requesting Android runtime permissions before startup: {missing:?}");
            if request_permissions(app, &missing) {
                *state = GateState::Waiting {
                    requested_at: Instant::now(),
                    saw_pause: false,
                };
                false
            } else {
                log::warn!("Failed to invoke Activity.requestPermissions; continuing without mic permission");
                *state = GateState::Ready;
                true
            }
        }
        GateState::Waiting { requested_at, saw_pause } => {
            if from_resumed && saw_pause {
                log::info!("Android permission dialog completed (remaining ungranted: {missing:?}); starting viewer");
                *state = GateState::Ready;
                true
            } else if !saw_pause && requested_at.elapsed() >= Duration::from_millis(1500) {
                log::info!("Android permission request completed without system dialog (ungranted: {missing:?}); starting viewer");
                *state = GateState::Ready;
                true
            } else {
                false
            }
        }
        GateState::Ready => true,
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
union JValue {
    i: i32,
    j: i64,
    l: *mut c_void,
}

type GetEnvFn = unsafe extern "C" fn(*mut c_void, *mut *mut c_void, i32) -> i32;
type AttachCurrentThreadFn = unsafe extern "C" fn(*mut c_void, *mut *mut c_void, *mut c_void) -> i32;
type FindClassFn = unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void;
type ExceptionClearFn = unsafe extern "C" fn(*mut c_void);
type DeleteLocalRefFn = unsafe extern "C" fn(*mut c_void, *mut c_void);
type GetObjectClassFn = unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void;
type GetMethodIdFn = unsafe extern "C" fn(*mut c_void, *mut c_void, *const c_char, *const c_char) -> *mut c_void;
type CallIntMethodAFn = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *const JValue) -> i32;
type CallVoidMethodAFn = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *const JValue);
type NewStringUtfFn = unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void;
type NewObjectArrayFn = unsafe extern "C" fn(*mut c_void, i32, *mut c_void, *mut c_void) -> *mut c_void;
type SetObjectArrayElementFn = unsafe extern "C" fn(*mut c_void, *mut c_void, i32, *mut c_void);
type ExceptionCheckFn = unsafe extern "C" fn(*mut c_void) -> u8;

unsafe fn attach_jni_env(app: &AndroidApp) -> Option<*mut c_void> {
    let vm = app.vm_as_ptr();
    if vm.is_null() {
        return None;
    }
    // SAFETY: `vm` is a valid `JavaVM*` supplied by `AndroidApp`; its first word
    // points to the standard `JNIInvokeInterface` function table (slots 4 and 6).
    unsafe {
        let vtable = *(vm as *const *const *const c_void);
        if vtable.is_null() {
            return None;
        }
        let get_env: GetEnvFn = std::mem::transmute(*vtable.add(6));
        let mut env: *mut c_void = std::ptr::null_mut();
        if get_env(vm, &mut env, JNI_VERSION_1_6) == 0 && !env.is_null() {
            return Some(env);
        }
        let attach: AttachCurrentThreadFn = std::mem::transmute(*vtable.add(4));
        if attach(vm, &mut env, std::ptr::null_mut()) == 0 && !env.is_null() {
            Some(env)
        } else {
            None
        }
    }
}

unsafe fn clear_exception(env: *mut c_void, vtable: *const *const c_void) -> bool {
    // SAFETY: `env` and `vtable` belong to the attached `JNIEnv*`; slots 228 and 17
    // are `ExceptionCheck` and `ExceptionClear` in `JNINativeInterface`.
    unsafe {
        let check: ExceptionCheckFn = std::mem::transmute(*vtable.add(228));
        if check(env) != 0 {
            let clear: ExceptionClearFn = std::mem::transmute(*vtable.add(17));
            clear(env);
            true
        } else {
            false
        }
    }
}

fn check_self_permission(app: &AndroidApp, permission: &str) -> bool {
    let Ok(perm_c) = CString::new(permission) else {
        return false;
    };
    let activity = app.activity_as_ptr();
    if activity.is_null() {
        return true;
    }
    // SAFETY: Calls `Activity.checkSelfPermission(String)` via the standard JNI
    // function table on the attached thread and deletes all local references.
    unsafe {
        let Some(env) = attach_jni_env(app) else {
            return true;
        };
        let vtable = *(env as *const *const *const c_void);
        let get_object_class: GetObjectClassFn = std::mem::transmute(*vtable.add(31));
        let get_method_id: GetMethodIdFn = std::mem::transmute(*vtable.add(33));
        let new_string_utf: NewStringUtfFn = std::mem::transmute(*vtable.add(167));
        let call_int_method_a: CallIntMethodAFn = std::mem::transmute(*vtable.add(51));
        let delete_local_ref: DeleteLocalRefFn = std::mem::transmute(*vtable.add(23));

        let cls = get_object_class(env, activity);
        if cls.is_null() || clear_exception(env, vtable) {
            return true;
        }
        let mid = get_method_id(env, cls, c"checkSelfPermission".as_ptr(), c"(Ljava/lang/String;)I".as_ptr());
        if mid.is_null() || clear_exception(env, vtable) {
            delete_local_ref(env, cls);
            return true;
        }
        let jperm = new_string_utf(env, perm_c.as_ptr());
        if jperm.is_null() || clear_exception(env, vtable) {
            delete_local_ref(env, cls);
            return true;
        }
        let args = [JValue { l: jperm }];
        let res = call_int_method_a(env, activity, mid, args.as_ptr());
        let had_exc = clear_exception(env, vtable);
        delete_local_ref(env, jperm);
        delete_local_ref(env, cls);
        !had_exc && res == 0
    }
}

fn request_permissions(app: &AndroidApp, permissions: &[&str]) -> bool {
    let activity = app.activity_as_ptr();
    if activity.is_null() || permissions.is_empty() {
        return false;
    }
    // SAFETY: Calls `Activity.requestPermissions(String[], int)` via the standard
    // JNI function table on the attached thread and deletes all local references.
    unsafe {
        let Some(env) = attach_jni_env(app) else {
            return false;
        };
        let vtable = *(env as *const *const *const c_void);
        let find_class: FindClassFn = std::mem::transmute(*vtable.add(6));
        let get_object_class: GetObjectClassFn = std::mem::transmute(*vtable.add(31));
        let get_method_id: GetMethodIdFn = std::mem::transmute(*vtable.add(33));
        let new_string_utf: NewStringUtfFn = std::mem::transmute(*vtable.add(167));
        let new_object_array: NewObjectArrayFn = std::mem::transmute(*vtable.add(172));
        let set_object_array_element: SetObjectArrayElementFn = std::mem::transmute(*vtable.add(174));
        let call_void_method_a: CallVoidMethodAFn = std::mem::transmute(*vtable.add(63));
        let delete_local_ref: DeleteLocalRefFn = std::mem::transmute(*vtable.add(23));

        let string_cls = find_class(env, CStr::from_bytes_with_nul_unchecked(b"java/lang/String\0").as_ptr());
        if string_cls.is_null() || clear_exception(env, vtable) {
            return false;
        }
        let arr = new_object_array(env, permissions.len() as i32, string_cls, std::ptr::null_mut());
        delete_local_ref(env, string_cls);
        if arr.is_null() || clear_exception(env, vtable) {
            return false;
        }
        for (i, &perm) in permissions.iter().enumerate() {
            let Ok(perm_c) = CString::new(perm) else {
                delete_local_ref(env, arr);
                return false;
            };
            let jperm = new_string_utf(env, perm_c.as_ptr());
            if jperm.is_null() || clear_exception(env, vtable) {
                delete_local_ref(env, arr);
                return false;
            }
            set_object_array_element(env, arr, i as i32, jperm);
            delete_local_ref(env, jperm);
            if clear_exception(env, vtable) {
                delete_local_ref(env, arr);
                return false;
            }
        }

        let cls = get_object_class(env, activity);
        if cls.is_null() || clear_exception(env, vtable) {
            delete_local_ref(env, arr);
            return false;
        }
        let mid = get_method_id(env, cls, c"requestPermissions".as_ptr(), c"([Ljava/lang/String;I)V".as_ptr());
        if mid.is_null() || clear_exception(env, vtable) {
            delete_local_ref(env, cls);
            delete_local_ref(env, arr);
            return false;
        }
        let args = [
            JValue { l: arr },
            JValue {
                i: PERMISSION_REQUEST_CODE,
            },
        ];
        call_void_method_a(env, activity, mid, args.as_ptr());
        let had_exc = clear_exception(env, vtable);
        delete_local_ref(env, cls);
        delete_local_ref(env, arr);
        !had_exc
    }
}

/// Requests sensor-landscape orientation (`true`) for Mobile VR or restores
/// user/unspecified orientation (`false`).
pub(crate) fn set_landscape(landscape: bool) {
    let Some(app) = ANDROID_APP.get() else {
        return;
    };
    const SCREEN_ORIENTATION_UNSPECIFIED: i32 = -1;
    const SCREEN_ORIENTATION_SENSOR_LANDSCAPE: i32 = 6;
    let orientation = if landscape {
        SCREEN_ORIENTATION_SENSOR_LANDSCAPE
    } else {
        SCREEN_ORIENTATION_UNSPECIFIED
    };
    // SAFETY: `app` holds valid `JavaVM*` and `Activity` references; `setRequestedOrientation`
    // is safe to call from native threads via ActivityManagerService IPC.
    unsafe {
        let Some(env) = attach_jni_env(app) else {
            return;
        };
        let activity = app.activity_as_ptr();
        if activity.is_null() {
            return;
        }
        let vtable = *(env as *const *const *const c_void);
        let get_object_class: GetObjectClassFn = std::mem::transmute(*vtable.add(31));
        let get_method_id: GetMethodIdFn = std::mem::transmute(*vtable.add(33));
        let call_void_method_a: CallVoidMethodAFn = std::mem::transmute(*vtable.add(63));
        let delete_local_ref: DeleteLocalRefFn = std::mem::transmute(*vtable.add(23));

        let cls = get_object_class(env, activity);
        if cls.is_null() || clear_exception(env, vtable) {
            return;
        }
        let mid = get_method_id(env, cls, c"setRequestedOrientation".as_ptr(), c"(I)V".as_ptr());
        if !mid.is_null() && !clear_exception(env, vtable) {
            let args = [JValue { i: orientation }];
            call_void_method_a(env, activity, mid, args.as_ptr());
            let _ = clear_exception(env, vtable);
        }
        delete_local_ref(env, cls);
    }
}

fn call_activity_void_method(method_name: &CStr) {
    let Some(app) = ANDROID_APP.get() else {
        return;
    };
    // SAFETY: `app` holds valid `JavaVM*` and `Activity` references; the target
    // zero-arg void method on `AuroraActivity` posts its UI work via `runOnUiThread`.
    unsafe {
        let Some(env) = attach_jni_env(app) else {
            return;
        };
        let activity = app.activity_as_ptr();
        if activity.is_null() {
            return;
        }
        let vtable = *(env as *const *const *const c_void);
        let get_object_class: GetObjectClassFn = std::mem::transmute(*vtable.add(31));
        let get_method_id: GetMethodIdFn = std::mem::transmute(*vtable.add(33));
        let call_void_method_a: CallVoidMethodAFn = std::mem::transmute(*vtable.add(63));
        let delete_local_ref: DeleteLocalRefFn = std::mem::transmute(*vtable.add(23));

        let cls = get_object_class(env, activity);
        if cls.is_null() || clear_exception(env, vtable) {
            return;
        }
        let mid = get_method_id(env, cls, method_name.as_ptr(), c"()V".as_ptr());
        if !mid.is_null() && !clear_exception(env, vtable) {
            call_void_method_a(env, activity, mid, std::ptr::null());
            let _ = clear_exception(env, vtable);
        }
        delete_local_ref(env, cls);
    }
}

/// Updates the native Android startup loading overlay progress bar and status text.
pub(crate) fn update_loading_progress(percent: i32, status: &str) {
    if LOADING_HIDDEN.load(Ordering::Relaxed) {
        return;
    }
    let Some(app) = ANDROID_APP.get() else {
        return;
    };
    let Ok(status_c) = CString::new(status) else {
        return;
    };
    // SAFETY: `app` holds valid `JavaVM*` and `Activity` references; `updateLoadingProgress`
    // posts its UI updates via `runOnUiThread`.
    unsafe {
        let Some(env) = attach_jni_env(app) else {
            return;
        };
        let activity = app.activity_as_ptr();
        if activity.is_null() {
            return;
        }
        let vtable = *(env as *const *const *const c_void);
        let get_object_class: GetObjectClassFn = std::mem::transmute(*vtable.add(31));
        let get_method_id: GetMethodIdFn = std::mem::transmute(*vtable.add(33));
        let call_void_method_a: CallVoidMethodAFn = std::mem::transmute(*vtable.add(63));
        let delete_local_ref: DeleteLocalRefFn = std::mem::transmute(*vtable.add(23));
        let new_string_utf: NewStringUtfFn = std::mem::transmute(*vtable.add(167));

        let jstatus = new_string_utf(env, status_c.as_ptr());
        if jstatus.is_null() || clear_exception(env, vtable) {
            return;
        }
        let cls = get_object_class(env, activity);
        if cls.is_null() || clear_exception(env, vtable) {
            delete_local_ref(env, jstatus);
            return;
        }
        let mid = get_method_id(env, cls, c"updateLoadingProgress".as_ptr(), c"(ILjava/lang/String;)V".as_ptr());
        if !mid.is_null() && !clear_exception(env, vtable) {
            let args = [JValue { i: percent }, JValue { l: jstatus }];
            call_void_method_a(env, activity, mid, args.as_ptr());
            let _ = clear_exception(env, vtable);
        }
        delete_local_ref(env, cls);
        delete_local_ref(env, jstatus);
    }
}

/// Dismisses the native Android startup loading overlay once the first frame is ready.
pub(crate) fn hide_loading_overlay() {
    if LOADING_HIDDEN.swap(true, Ordering::Relaxed) {
        return;
    }
    call_activity_void_method(c"hideLoadingOverlay");
}

/// Latest Android safe-area insets (`[top, bottom, left, right]`) and IME
/// bottom height, all in physical pixels.
pub(crate) fn window_insets_px() -> ([f32; 4], f32) {
    let safe = [
        SAFE_INSETS_PX[0].load(Ordering::Relaxed) as f32,
        SAFE_INSETS_PX[1].load(Ordering::Relaxed) as f32,
        SAFE_INSETS_PX[2].load(Ordering::Relaxed) as f32,
        SAFE_INSETS_PX[3].load(Ordering::Relaxed) as f32,
    ];
    let ime = IME_BOTTOM_PX.load(Ordering::Relaxed) as f32;
    (safe, ime)
}

/// Synchronizes the Android soft keyboard with egui's text focus state.
///
/// Uses a short debounce before hiding so a 1-frame focus flicker (for example
/// while tapping a `TextEdit` or resizing a mobile sheet) does not dismiss the IME.
pub(crate) fn sync_soft_keyboard(wants_keyboard: bool) {
    static HIDE_STREAK: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
    if wants_keyboard {
        HIDE_STREAK.store(0, Ordering::Relaxed);
        if !SOFT_KEYBOARD_SHOWN.swap(true, Ordering::Relaxed) {
            show_soft_keyboard();
        }
    } else if SOFT_KEYBOARD_SHOWN.load(Ordering::Relaxed) {
        let streak = HIDE_STREAK.fetch_add(1, Ordering::Relaxed).saturating_add(1);
        if streak >= 4 {
            HIDE_STREAK.store(0, Ordering::Relaxed);
            SOFT_KEYBOARD_SHOWN.store(false, Ordering::Relaxed);
            hide_soft_keyboard();
        }
    } else {
        HIDE_STREAK.store(0, Ordering::Relaxed);
    }
}

/// Shows the Android soft keyboard (IME).
pub(crate) fn show_soft_keyboard() {
    call_activity_void_method(c"showSoftKeyboard");
}

/// Hides the Android soft keyboard (IME).
pub(crate) fn hide_soft_keyboard() {
    call_activity_void_method(c"hideSoftKeyboard");
}

/// Triggers a short native Android haptic feedback pulse (`0` = tap, `1` = long-press, `2` = confirm).
pub(crate) fn perform_haptic(kind: i32) {
    let Some(app) = ANDROID_APP.get() else {
        return;
    };
    // SAFETY: `app` holds valid `JavaVM*` and `Activity` references;
    // `performHapticFeedback(int)` posts its UI work via `runOnUiThread`.
    unsafe {
        let Some(env) = attach_jni_env(app) else {
            return;
        };
        let activity = app.activity_as_ptr();
        if activity.is_null() {
            return;
        }
        let vtable = *(env as *const *const *const c_void);
        let get_object_class: GetObjectClassFn = std::mem::transmute(*vtable.add(31));
        let get_method_id: GetMethodIdFn = std::mem::transmute(*vtable.add(33));
        let call_void_method_a: CallVoidMethodAFn = std::mem::transmute(*vtable.add(63));
        let delete_local_ref: DeleteLocalRefFn = std::mem::transmute(*vtable.add(23));

        let cls = get_object_class(env, activity);
        if cls.is_null() || clear_exception(env, vtable) {
            return;
        }
        let mid = get_method_id(env, cls, c"performHapticFeedback".as_ptr(), c"(I)V".as_ptr());
        if !mid.is_null() && !clear_exception(env, vtable) {
            let args = [JValue { i: kind }];
            call_void_method_a(env, activity, mid, args.as_ptr());
            let _ = clear_exception(env, vtable);
        }
        delete_local_ref(env, cls);
    }
}

fn call_activity_string_method(method: &CStr, arg1: &str, arg2: Option<&str>) {
    let Some(app) = ANDROID_APP.get() else {
        return;
    };
    let Ok(c_arg1) = CString::new(arg1) else {
        return;
    };
    let c_arg2 = match arg2 {
        Some(s) => match CString::new(s) {
            Ok(c) => Some(c),
            Err(_) => return,
        },
        None => None,
    };
    // SAFETY: `app` holds valid `JavaVM*` and `Activity` references; all JNI
    // local references created here are deleted before returning.
    unsafe {
        let Some(env) = attach_jni_env(app) else {
            return;
        };
        let activity = app.activity_as_ptr();
        if activity.is_null() {
            return;
        }
        let vtable = *(env as *const *const *const c_void);
        let get_object_class: GetObjectClassFn = std::mem::transmute(*vtable.add(31));
        let get_method_id: GetMethodIdFn = std::mem::transmute(*vtable.add(33));
        let new_string_utf: NewStringUtfFn = std::mem::transmute(*vtable.add(167));
        let call_void_method_a: CallVoidMethodAFn = std::mem::transmute(*vtable.add(63));
        let delete_local_ref: DeleteLocalRefFn = std::mem::transmute(*vtable.add(23));

        let cls = get_object_class(env, activity);
        if cls.is_null() || clear_exception(env, vtable) {
            return;
        }
        let sig = if c_arg2.is_some() {
            c"(Ljava/lang/String;Ljava/lang/String;)V"
        } else {
            c"(Ljava/lang/String;)V"
        };
        let mid = get_method_id(env, cls, method.as_ptr(), sig.as_ptr());
        if !mid.is_null() && !clear_exception(env, vtable) {
            let j1 = new_string_utf(env, c_arg1.as_ptr());
            if !j1.is_null() && !clear_exception(env, vtable) {
                if let Some(ref c2) = c_arg2 {
                    let j2 = new_string_utf(env, c2.as_ptr());
                    if !j2.is_null() && !clear_exception(env, vtable) {
                        let args = [JValue { l: j1 }, JValue { l: j2 }];
                        call_void_method_a(env, activity, mid, args.as_ptr());
                        let _ = clear_exception(env, vtable);
                    }
                    if !j2.is_null() {
                        delete_local_ref(env, j2);
                    }
                } else {
                    let args = [JValue { l: j1 }];
                    call_void_method_a(env, activity, mid, args.as_ptr());
                    let _ = clear_exception(env, vtable);
                }
            }
            if !j1.is_null() {
                delete_local_ref(env, j1);
            }
        }
        delete_local_ref(env, cls);
    }
}

/// Logs a breadcrumb message to Firebase Crashlytics when configured.
pub(crate) fn crashlytics_log(message: &str) {
    call_activity_string_method(c"crashlyticsLog", message, None);
}

/// Sets a custom diagnostic key/value in Firebase Crashlytics when configured.
pub(crate) fn crashlytics_set_key(key: &str, value: &str) {
    call_activity_string_method(c"crashlyticsSetKey", key, Some(value));
}

/// Records a non-fatal / panic diagnostic event in Firebase Crashlytics when configured.
pub(crate) fn crashlytics_record_error(message: &str) {
    call_activity_string_method(c"crashlyticsRecordError", message, None);
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
            .unwrap_or_else(|| "unknown".to_owned());
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_owned()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "non-string panic payload".to_owned()
        };
        let summary = format!("Rust panic at {location}: {payload}");
        crashlytics_set_key("panic_location", &location);
        crashlytics_log(&summary);
        crashlytics_record_error(&summary);
        previous(info);
    }));
}

#[unsafe(no_mangle)]
pub extern "C" fn Java_org_auroraviewer_aurora_AuroraActivity_nativeOnWindowInsets(
    _env: *mut c_void,
    _class: *mut c_void,
    top: i32,
    bottom: i32,
    left: i32,
    right: i32,
    ime_bottom: i32,
) {
    SAFE_INSETS_PX[0].store(top.max(0) as u32, Ordering::Relaxed);
    SAFE_INSETS_PX[1].store(bottom.max(0) as u32, Ordering::Relaxed);
    SAFE_INSETS_PX[2].store(left.max(0) as u32, Ordering::Relaxed);
    SAFE_INSETS_PX[3].store(right.max(0) as u32, Ordering::Relaxed);
    let new_ime = ime_bottom.max(0) as u32;
    let prev_ime = IME_BOTTOM_PX.swap(new_ime, Ordering::Relaxed);
    if prev_ime > 0 && new_ime == 0 {
        SOFT_KEYBOARD_SHOWN.store(false, Ordering::Relaxed);
    }
}

/// Latest Android gamepad / Bluetooth VR remote state: `(seen, [lx, ly, rx, ry], key_mask)`.
pub(crate) fn gamepad_state() -> (bool, [f32; 4], u32) {
    let seen = GAMEPAD_SEEN.load(Ordering::Relaxed);
    let axes = [
        f32::from_bits(GAMEPAD_AXES[0].load(Ordering::Relaxed)),
        f32::from_bits(GAMEPAD_AXES[1].load(Ordering::Relaxed)),
        f32::from_bits(GAMEPAD_AXES[2].load(Ordering::Relaxed)),
        f32::from_bits(GAMEPAD_AXES[3].load(Ordering::Relaxed)),
    ];
    let keys = GAMEPAD_KEYS.load(Ordering::Relaxed);
    (seen, axes, keys)
}

#[unsafe(no_mangle)]
pub extern "C" fn Java_org_auroraviewer_aurora_AuroraActivity_nativeOnGamepadAxis(
    _env: *mut c_void,
    _class: *mut c_void,
    lx: f32,
    ly: f32,
    rx: f32,
    ry: f32,
) {
    GAMEPAD_SEEN.store(true, Ordering::Relaxed);
    GAMEPAD_AXES[0].store(lx.to_bits(), Ordering::Relaxed);
    GAMEPAD_AXES[1].store(ly.to_bits(), Ordering::Relaxed);
    GAMEPAD_AXES[2].store(rx.to_bits(), Ordering::Relaxed);
    GAMEPAD_AXES[3].store(ry.to_bits(), Ordering::Relaxed);
}

#[unsafe(no_mangle)]
pub extern "C" fn Java_org_auroraviewer_aurora_AuroraActivity_nativeOnGamepadKey(
    _env: *mut c_void,
    _class: *mut c_void,
    key_code: i32,
    down: u8,
) {
    let bit = match key_code {
        96 | 23 | 66 => 1 << 0, // BUTTON_A / DPAD_CENTER / ENTER -> South
        97 => 1 << 1,           // BUTTON_B -> East
        100 => 1 << 2,          // BUTTON_Y -> North
        99 => 1 << 3,           // BUTTON_X -> West
        102 => 1 << 4,          // BUTTON_L1 -> LeftBumper
        103 => 1 << 5,          // BUTTON_R1 -> RightBumper
        19 => 1 << 6,           // DPAD_UP
        20 => 1 << 7,           // DPAD_DOWN
        21 => 1 << 8,           // DPAD_LEFT
        22 => 1 << 9,           // DPAD_RIGHT
        _ => return,
    };
    GAMEPAD_SEEN.store(true, Ordering::Relaxed);
    if down != 0 {
        GAMEPAD_KEYS.fetch_or(bit, Ordering::Relaxed);
    } else {
        GAMEPAD_KEYS.fetch_and(!bit, Ordering::Relaxed);
    }
}

/// Controls whether Android's Back button should be intercepted to exit VR/mouselook.
pub(crate) fn set_back_consumed(consumed: bool) {
    BACK_CONSUMED.store(consumed, Ordering::Relaxed);
}

/// Returns `true` once if Android's Back button was pressed while intercepted.
pub(crate) fn take_back_pressed() -> bool {
    BACK_PRESSED.swap(false, Ordering::Relaxed)
}

#[unsafe(no_mangle)]
pub extern "C" fn Java_org_auroraviewer_aurora_AuroraActivity_nativeOnBackPressed(_env: *mut c_void, _class: *mut c_void) -> u8 {
    if BACK_CONSUMED.load(Ordering::Relaxed) {
        BACK_PRESSED.store(true, Ordering::Relaxed);
        1
    } else {
        0
    }
}

fn init_tls_platform_verifier(app: &AndroidApp) {
    let vm_ptr = app.vm_as_ptr();
    let activity = app.activity_as_ptr();
    if vm_ptr.is_null() || activity.is_null() {
        log::warn!("Android TLS verifier init skipped: missing JavaVM or Activity pointer");
        return;
    }
    // SAFETY: `vm_ptr` is a valid `JavaVM*` supplied by `AndroidApp` for the
    // lifetime of `android_main`.
    let vm = unsafe { jni::JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM) };
    let res = vm.attach_current_thread(|env| -> jni::errors::Result<()> {
        // SAFETY: `activity` is a valid `jobject` (`android.app.Activity`, a `Context`)
        // alive for the duration of `android_main`.
        let context = unsafe { jni::objects::JObject::from_raw(env, activity as jni::sys::jobject) };
        rustls_platform_verifier::android::init_with_env(env, context)
    });
    if let Err(err) = res {
        log::warn!("Failed to initialize rustls-platform-verifier on Android: {err}");
    }
}

#[unsafe(no_mangle)]
pub fn android_main(app: AndroidApp) {
    if let Some(dir) = app.internal_data_path().or_else(|| app.external_data_path()) {
        let _ = std::fs::create_dir_all(&dir);
        let _ = DATA_DIR.set(dir);
    }
    let _ = ANDROID_APP.set(app.clone());
    aurora_i18n::select("default", None);
    crate::logging::init();
    install_panic_hook();
    crashlytics_log("android_main: start");
    init_tls_platform_verifier(&app);
    if let Err(error) = crate::run_viewer_with_android(Some(app)) {
        log::error!("Android viewer error: {error:#}");
        crashlytics_record_error(&format!("Android viewer error: {error:#}"));
    }
    crate::logging::drain(crate::logging::DRAIN_WAIT);
    // Terminate the process so a subsequent Activity launch does not inherit
    // winit's static OnceLock EventLoop state ("EventLoop can't be recreated").
    std::process::exit(0);
}
