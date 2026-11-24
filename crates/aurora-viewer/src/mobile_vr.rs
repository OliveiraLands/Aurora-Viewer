//! Direct mobile VR: 3DoF head tracking from Android device sensors
//! (`ASensorManager` / `ASensorEventQueue`) and stereo eye offset helpers.

use glam::{Quat, Vec3};
use std::time::Instant;

/// Default interpupillary distance in millimetres for mobile VR headsets.
pub const DEFAULT_IPD_MM: f32 = 63.0;

/// Direct mobile VR state (cardboard / phone-in-headset mode).
#[allow(dead_code)]
pub struct MobileVr {
    pub enabled: bool,
    pub ipd_mm: f32,
    recenter_yaw: f32,
    needs_recenter: bool,
    orientation: Quat,
    last_poll: Option<Instant>,
    #[cfg(target_os = "android")]
    sensors: Option<android_sensors::SensorReader>,
}

impl Default for MobileVr {
    fn default() -> Self {
        Self {
            enabled: false,
            ipd_mm: DEFAULT_IPD_MM,
            recenter_yaw: 0.0,
            needs_recenter: false,
            orientation: Quat::IDENTITY,
            last_poll: None,
            #[cfg(target_os = "android")]
            sensors: None,
        }
    }
}

impl MobileVr {
    /// Enable or disable direct mobile VR, starting/stopping sensor polling.
    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled == enabled {
            return;
        }
        self.enabled = enabled;
        self.last_poll = None;
        self.needs_recenter = enabled;
        #[cfg(target_os = "android")]
        {
            if enabled {
                if self.sensors.is_none() {
                    self.sensors = android_sensors::SensorReader::open();
                }
                if let Some(s) = &mut self.sensors {
                    s.enable();
                }
            } else if let Some(s) = &mut self.sensors {
                s.disable();
            }
        }
    }

    /// Recenter the horizontal yaw so the current phone direction is forward.
    #[allow(dead_code)]
    pub fn recenter(&mut self) {
        self.recenter_yaw = extract_yaw(self.orientation);
    }

    /// Poll device sensors and return a fresh `HeadPose` when Mobile VR is active.
    pub fn poll_head_pose(&mut self, now: Instant) -> Option<aurora_render::HeadPose> {
        if !self.enabled {
            return None;
        }
        let dt = self.last_poll.map_or(1.0 / 60.0, |prev| {
            now.saturating_duration_since(prev).as_secs_f32().clamp(0.001, 0.1)
        });
        self.last_poll = Some(now);
        #[cfg(target_os = "android")]
        if let Some(s) = &mut self.sensors {
            let next = s.poll(self.orientation, dt);
            if next != self.orientation {
                self.orientation = next;
                if self.needs_recenter {
                    self.needs_recenter = false;
                    self.recenter_yaw = extract_yaw(self.orientation);
                }
            }
        }
        #[cfg(not(target_os = "android"))]
        let _ = dt;

        let rotation = apply_yaw_recenter(self.orientation, self.recenter_yaw);
        Some(aurora_render::HeadPose {
            rotation,
            position: Vec3::ZERO,
            rig_position: Vec3::ZERO,
            generation: 1,
            origin: None,
            revision: 1,
            sampled: now,
            mode: aurora_render::stereo_view::RigMode::FirstPerson,
        })
    }
}

/// Convert an Android landscape rotation-vector quaternion `(x, y, z, w)` to
/// Aurora's Y-up / Z-back head-pose quaternion.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub fn sensor_quat_to_head_pose(x: f32, y: f32, z: f32, w: f32) -> Quat {
    let q = Quat::from_xyzw(x, y, z, w);
    if q.length_squared() < 1e-6 {
        return Quat::IDENTITY;
    }
    // Remap Android world (Z-up) to viewer head space (Y-up, Z-back) and
    // undo the 90 deg CCW landscape rotation of the phone's natural sensor frame.
    let world_to_view = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    let landscape_to_portrait = Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2);
    (world_to_view * q.normalize() * landscape_to_portrait).normalize()
}

/// Complementary sensor fusion step combining angular velocity (rad/s) and
/// accelerometer gravity vector (m/s^2) when a hardware rotation vector is absent.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub fn fuse_gyro_accel(current: Quat, gyro_rad_s: Vec3, accel_m_s2: Vec3, dt: f32) -> Quat {
    let angle = gyro_rad_s.length() * dt;
    let predicted = if angle > 1e-6 {
        (current * Quat::from_axis_angle(gyro_rad_s.normalize(), angle)).normalize()
    } else {
        current
    };
    let acc_len = accel_m_s2.length();
    if !(5.0..=15.0).contains(&acc_len) {
        return predicted;
    }
    let up_meas = accel_m_s2 / acc_len;
    let up_pred = predicted * Vec3::Y;
    let corr = Quat::from_rotation_arc(up_pred, up_meas);
    predicted.slerp((corr * predicted).normalize(), 0.02).normalize()
}

/// Extract horizontal yaw (radians) around world Y from a head orientation.
#[allow(dead_code)]
pub fn extract_yaw(q: Quat) -> f32 {
    let fwd = q * Vec3::NEG_Z;
    fwd.x.atan2(-fwd.z)
}

/// Remove `yaw_offset` around world Y while keeping pitch and roll intact.
pub fn apply_yaw_recenter(q: Quat, yaw_offset: f32) -> Quat {
    (Quat::from_rotation_y(yaw_offset) * q).normalize()
}

/// Left and right eye horizontal offsets in metres for a given IPD in mm.
#[allow(dead_code)]
pub fn eye_offsets(ipd_mm: f32) -> (Vec3, Vec3) {
    let half = (ipd_mm.clamp(50.0, 80.0) * 0.001) * 0.5;
    (Vec3::new(-half, 0.0, 0.0), Vec3::new(half, 0.0, 0.0))
}

#[cfg(target_os = "android")]
mod android_sensors {
    use super::*;
    use std::ffi::{c_char, c_int, c_void};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Condvar, Mutex};

    const ASENSOR_TYPE_ACCELEROMETER: c_int = 1;
    const ASENSOR_TYPE_GYROSCOPE: c_int = 4;
    const ASENSOR_TYPE_ROTATION_VECTOR: c_int = 11;
    const ASENSOR_TYPE_GAME_ROTATION_VECTOR: c_int = 15;
    const LOOPER_PREPARE_ALLOW_NON_CALLBACKS: c_int = 1;
    // 200 Hz (5000 microseconds)
    const SENSOR_RATE_US: c_int = 5_000;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct ASensorVector {
        v: [f32; 4],
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    union ASensorEventData {
        vector: ASensorVector,
        raw: [u8; 64],
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct ASensorEvent {
        version: i32,
        sensor: i32,
        event_type: i32,
        reserved0: i32,
        timestamp: i64,
        data: ASensorEventData,
        reserved1: [u64; 4],
    }

    unsafe extern "C" {
        fn ALooper_prepare(opts: c_int) -> *mut c_void;
        fn ALooper_pollOnce(timeout_millis: c_int, out_fd: *mut c_int, out_events: *mut c_int, out_data: *mut *mut c_void) -> c_int;
        fn ASensorManager_getInstanceForPackage(package_name: *const c_char) -> *mut c_void;
        fn ASensorManager_getDefaultSensor(manager: *mut c_void, sensor_type: c_int) -> *const c_void;
        fn ASensorManager_createEventQueue(
            manager: *mut c_void,
            looper: *mut c_void,
            ident: c_int,
            callback: *const c_void,
            data: *mut c_void,
        ) -> *mut c_void;
        fn ASensorManager_destroyEventQueue(manager: *mut c_void, queue: *mut c_void) -> c_int;
        fn ASensorEventQueue_enableSensor(queue: *mut c_void, sensor: *const c_void) -> c_int;
        fn ASensorEventQueue_disableSensor(queue: *mut c_void, sensor: *const c_void) -> c_int;
        fn ASensorEventQueue_setEventRate(queue: *mut c_void, sensor: *const c_void, usec: i32) -> c_int;
        fn ASensorEventQueue_getEvents(queue: *mut c_void, events: *mut ASensorEvent, count: usize) -> isize;
    }

    struct Shared {
        running: AtomicBool,
        enabled: Mutex<bool>,
        wake: Condvar,
        orientation: Mutex<Quat>,
    }

    pub(super) struct SensorReader {
        shared: Arc<Shared>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl SensorReader {
        /// Spawns a dedicated thread with its own `ALooper` so `ASensorEventQueue`
        /// never shares or wakes `android_main`'s `winit` / `android_activity` `ALooper`.
        pub(super) fn open() -> Option<Self> {
            let shared = Arc::new(Shared {
                running: AtomicBool::new(true),
                enabled: Mutex::new(false),
                wake: Condvar::new(),
                orientation: Mutex::new(Quat::IDENTITY),
            });
            let worker = Arc::clone(&shared);
            let thread = std::thread::Builder::new()
                .name("aurora-vr-sensors".into())
                .spawn(move || sensor_thread(worker))
                .ok()?;
            Some(Self {
                shared,
                thread: Some(thread),
            })
        }

        pub(super) fn enable(&mut self) {
            if let Ok(mut on) = self.shared.enabled.lock() {
                *on = true;
                self.shared.wake.notify_all();
            }
        }

        pub(super) fn disable(&mut self) {
            if let Ok(mut on) = self.shared.enabled.lock() {
                *on = false;
                self.shared.wake.notify_all();
            }
        }

        pub(super) fn poll(&mut self, current: Quat, _dt: f32) -> Quat {
            self.shared.orientation.lock().map(|q| *q).unwrap_or(current)
        }
    }

    impl Drop for SensorReader {
        fn drop(&mut self) {
            self.shared.running.store(false, Ordering::Relaxed);
            self.shared.wake.notify_all();
            if let Some(handle) = self.thread.take() {
                let _ = handle.join();
            }
        }
    }

    fn sensor_thread(shared: Arc<Shared>) {
        // SAFETY: Runs on its own thread with a thread-local `ALooper`; all NDK
        // handles are created, polled, and destroyed on this same thread.
        unsafe {
            let looper = ALooper_prepare(LOOPER_PREPARE_ALLOW_NON_CALLBACKS);
            let pkg = std::ffi::CString::new(crate::brand::ANDROID_PACKAGE)
                .unwrap_or_else(|_| std::ffi::CString::from(c"org.auroraviewer.aurora"));
            let manager = ASensorManager_getInstanceForPackage(pkg.as_ptr());
            if manager.is_null() || looper.is_null() {
                log::warn!("mobile_vr: ASensorManager unavailable");
                return;
            }
            let queue = ASensorManager_createEventQueue(manager, looper, 1, std::ptr::null(), std::ptr::null_mut());
            if queue.is_null() {
                log::warn!("mobile_vr: failed to create ASensorEventQueue");
                return;
            }
            let mut rot_sensor = ASensorManager_getDefaultSensor(manager, ASENSOR_TYPE_GAME_ROTATION_VECTOR);
            if rot_sensor.is_null() {
                rot_sensor = ASensorManager_getDefaultSensor(manager, ASENSOR_TYPE_ROTATION_VECTOR);
            }
            let gyro_sensor = ASensorManager_getDefaultSensor(manager, ASENSOR_TYPE_GYROSCOPE);
            let accel_sensor = ASensorManager_getDefaultSensor(manager, ASENSOR_TYPE_ACCELEROMETER);

            let mut active = false;
            let mut current = Quat::IDENTITY;
            let mut last_accel = Vec3::Y * 9.81;
            let mut last_poll = Instant::now();
            let mut buf = [ASensorEvent {
                version: 0,
                sensor: 0,
                event_type: 0,
                reserved0: 0,
                timestamp: 0,
                data: ASensorEventData { raw: [0; 64] },
                reserved1: [0; 4],
            }; 32];

            while shared.running.load(Ordering::Relaxed) {
                let wanted = {
                    let Ok(mut guard) = shared.enabled.lock() else {
                        break;
                    };
                    while !*guard && shared.running.load(Ordering::Relaxed) {
                        if active {
                            for sensor in [rot_sensor, gyro_sensor, accel_sensor] {
                                if !sensor.is_null() {
                                    ASensorEventQueue_disableSensor(queue, sensor);
                                }
                            }
                            active = false;
                        }
                        let Ok(next) = shared.wake.wait(guard) else {
                            return;
                        };
                        guard = next;
                    }
                    *guard
                };
                if !shared.running.load(Ordering::Relaxed) {
                    break;
                }
                if wanted && !active {
                    if !rot_sensor.is_null() {
                        ASensorEventQueue_enableSensor(queue, rot_sensor);
                        ASensorEventQueue_setEventRate(queue, rot_sensor, SENSOR_RATE_US);
                    } else {
                        if !gyro_sensor.is_null() {
                            ASensorEventQueue_enableSensor(queue, gyro_sensor);
                            ASensorEventQueue_setEventRate(queue, gyro_sensor, SENSOR_RATE_US);
                        }
                        if !accel_sensor.is_null() {
                            ASensorEventQueue_enableSensor(queue, accel_sensor);
                            ASensorEventQueue_setEventRate(queue, accel_sensor, SENSOR_RATE_US);
                        }
                    }
                    active = true;
                    last_poll = Instant::now();
                }

                let _ = ALooper_pollOnce(10, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
                let n = ASensorEventQueue_getEvents(queue, buf.as_mut_ptr(), buf.len());
                if n > 0 {
                    let now = Instant::now();
                    let dt = now.saturating_duration_since(last_poll).as_secs_f32().clamp(0.001, 0.1);
                    last_poll = now;
                    for ev in &buf[..n as usize] {
                        let v = ev.data.vector.v;
                        match ev.event_type {
                            ASENSOR_TYPE_GAME_ROTATION_VECTOR | ASENSOR_TYPE_ROTATION_VECTOR => {
                                let w_sq = (1.0 - (v[0] * v[0] + v[1] * v[1] + v[2] * v[2])).max(0.0);
                                let w = if v[3].abs() > 1e-6 { v[3] } else { w_sq.sqrt() };
                                current = sensor_quat_to_head_pose(v[0], v[1], v[2], w);
                            }
                            ASENSOR_TYPE_ACCELEROMETER => {
                                last_accel = Vec3::new(v[0], v[1], v[2]);
                            }
                            ASENSOR_TYPE_GYROSCOPE => {
                                current = fuse_gyro_accel(current, Vec3::new(v[0], v[1], v[2]), last_accel, dt);
                            }
                            _ => {}
                        }
                    }
                    if let Ok(mut out) = shared.orientation.lock() {
                        *out = current;
                    }
                }
            }

            for sensor in [rot_sensor, gyro_sensor, accel_sensor] {
                if !sensor.is_null() {
                    ASensorEventQueue_disableSensor(queue, sensor);
                }
            }
            ASensorManager_destroyEventQueue(manager, queue);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaw_recenter_zeroes_horizontal_heading() {
        let q = Quat::from_rotation_y(1.1) * Quat::from_rotation_x(0.2);
        let yaw = extract_yaw(q);
        let recentered = apply_yaw_recenter(q, yaw);
        assert!(extract_yaw(recentered).abs() < 1e-3);
    }

    #[test]
    fn gyro_accel_fusion_updates_orientation_smoothly() {
        let q0 = Quat::IDENTITY;
        let q1 = fuse_gyro_accel(q0, Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 9.81, 0.0), 0.016);
        assert!(q1.is_normalized());
        assert!((q1.angle_between(q0) - 0.016).abs() < 0.01);
    }

    #[test]
    fn eye_offsets_are_symmetric_for_ipd() {
        let (l, r) = eye_offsets(64.0);
        assert!((l.x + r.x).abs() < 1e-6);
        assert!((r.x - l.x - 0.064).abs() < 1e-6);
    }
}
