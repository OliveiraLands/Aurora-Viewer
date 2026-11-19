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
            self.orientation = s.poll(self.orientation, dt);
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
    // Remap Android landscape sensor frame (X right, Y up-screen, Z out-of-screen)
    // to viewer head space.
    let remap = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    (remap * q.normalize()).normalize()
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

    pub(super) struct SensorReader {
        manager: *mut c_void,
        queue: *mut c_void,
        rot_sensor: *const c_void,
        gyro_sensor: *const c_void,
        accel_sensor: *const c_void,
        last_accel: Vec3,
    }

    // SAFETY: `SensorReader` is owned and polled on the main viewer thread only.
    unsafe impl Send for SensorReader {}

    impl SensorReader {
        pub(super) fn open() -> Option<Self> {
            // SAFETY: NDK sensor manager C API calls with valid pointers/package name.
            unsafe {
                let looper = ALooper_prepare(LOOPER_PREPARE_ALLOW_NON_CALLBACKS);
                let manager = ASensorManager_getInstanceForPackage(c"org.auroraviewer.aurora".as_ptr());
                if manager.is_null() || looper.is_null() {
                    log::warn!("mobile_vr: ASensorManager unavailable");
                    return None;
                }
                let queue = ASensorManager_createEventQueue(manager, looper, 1, std::ptr::null(), std::ptr::null_mut());
                if queue.is_null() {
                    log::warn!("mobile_vr: failed to create ASensorEventQueue");
                    return None;
                }
                let mut rot_sensor = ASensorManager_getDefaultSensor(manager, ASENSOR_TYPE_GAME_ROTATION_VECTOR);
                if rot_sensor.is_null() {
                    rot_sensor = ASensorManager_getDefaultSensor(manager, ASENSOR_TYPE_ROTATION_VECTOR);
                }
                let gyro_sensor = ASensorManager_getDefaultSensor(manager, ASENSOR_TYPE_GYROSCOPE);
                let accel_sensor = ASensorManager_getDefaultSensor(manager, ASENSOR_TYPE_ACCELEROMETER);
                Some(Self {
                    manager,
                    queue,
                    rot_sensor,
                    gyro_sensor,
                    accel_sensor,
                    last_accel: Vec3::Y * 9.81,
                })
            }
        }

        pub(super) fn enable(&mut self) {
            // SAFETY: `queue` and sensor pointers came from `ASensorManager`.
            unsafe {
                if !self.rot_sensor.is_null() {
                    ASensorEventQueue_enableSensor(self.queue, self.rot_sensor);
                    ASensorEventQueue_setEventRate(self.queue, self.rot_sensor, SENSOR_RATE_US);
                } else {
                    if !self.gyro_sensor.is_null() {
                        ASensorEventQueue_enableSensor(self.queue, self.gyro_sensor);
                        ASensorEventQueue_setEventRate(self.queue, self.gyro_sensor, SENSOR_RATE_US);
                    }
                    if !self.accel_sensor.is_null() {
                        ASensorEventQueue_enableSensor(self.queue, self.accel_sensor);
                        ASensorEventQueue_setEventRate(self.queue, self.accel_sensor, SENSOR_RATE_US);
                    }
                }
            }
        }

        pub(super) fn disable(&mut self) {
            // SAFETY: `queue` and sensor pointers came from `ASensorManager`.
            unsafe {
                for sensor in [self.rot_sensor, self.gyro_sensor, self.accel_sensor] {
                    if !sensor.is_null() {
                        ASensorEventQueue_disableSensor(self.queue, sensor);
                    }
                }
            }
        }

        pub(super) fn poll(&mut self, mut current: Quat, dt: f32) -> Quat {
            let mut buf = [ASensorEvent {
                version: 0,
                sensor: 0,
                event_type: 0,
                reserved0: 0,
                timestamp: 0,
                data: ASensorEventData { raw: [0; 64] },
                reserved1: [0; 4],
            }; 32];
            // SAFETY: `buf` is valid writable memory of 32 `ASensorEvent`s.
            let n = unsafe { ASensorEventQueue_getEvents(self.queue, buf.as_mut_ptr(), buf.len()) };
            if n <= 0 {
                return current;
            }
            for ev in &buf[..n as usize] {
                // SAFETY: `ev.event_type` identifies which union field is populated.
                let v = unsafe { ev.data.vector.v };
                match ev.event_type {
                    ASENSOR_TYPE_GAME_ROTATION_VECTOR | ASENSOR_TYPE_ROTATION_VECTOR => {
                        let w_sq = (1.0 - (v[0] * v[0] + v[1] * v[1] + v[2] * v[2])).max(0.0);
                        let w = if v[3].abs() > 1e-6 { v[3] } else { w_sq.sqrt() };
                        current = sensor_quat_to_head_pose(v[0], v[1], v[2], w);
                    }
                    ASENSOR_TYPE_ACCELEROMETER => {
                        self.last_accel = Vec3::new(v[0], v[1], v[2]);
                    }
                    ASENSOR_TYPE_GYROSCOPE => {
                        current = fuse_gyro_accel(current, Vec3::new(v[0], v[1], v[2]), self.last_accel, dt);
                    }
                    _ => {}
                }
            }
            current
        }
    }

    impl Drop for SensorReader {
        fn drop(&mut self) {
            self.disable();
            // SAFETY: `manager` and `queue` are valid NDK handles.
            unsafe {
                ASensorManager_destroyEventQueue(self.manager, self.queue);
            }
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
