//! バッテリー状態の I/O 層。macOS の IOPowerSources を呼出す。
//!
//! window titleで使う既存の `Battery` 型への変換と、popup専用の
//! estimated runtimeを同じraw snapshotから行う。pmsetへのfallbackは持たない。
use crate::parsers::{Battery, ChargingState};

#[derive(Debug, PartialEq, Eq)]
/// IOPowerSources が返す、バッテリー残り時間の意味を表す。
pub enum RuntimeEstimate {
    /// 残り時間を秒で表す。値は 0 秒以上に正規化される。
    Seconds(u64),
    /// macOS が残り時間を推定できない状態。
    Unknown,
    /// AC 接続などにより、残り時間を制限する必要がない状態。
    Unlimited,
    /// 電源情報そのものを取得できなかった状態。
    Unavailable,
}

#[derive(Debug, PartialEq, Eq)]
/// 1回の IOPowerSources 読み取り結果。
pub struct PowerSnapshot {
    /// 既存の window title 表示と共有するバッテリー状態。
    pub battery: Battery,
    /// popup に表示する残り時間の状態。
    pub runtime: RuntimeEstimate,
}

/// IOPowerSourcesから一度だけ電源状態を取得する。
pub fn get_snapshot() -> PowerSnapshot {
    #[cfg(target_os = "macos")]
    {
        get_snapshot_macos(true)
    }

    #[cfg(not(target_os = "macos"))]
    {
        PowerSnapshot {
            battery: Battery::Absent,
            runtime: RuntimeEstimate::Unavailable,
        }
    }
}

/// 既存watch経路向けのBattery取得。表示契約は変更しない。
pub fn get() -> Battery {
    #[cfg(target_os = "macos")]
    {
        get_snapshot_macos(false).battery
    }

    #[cfg(not(target_os = "macos"))]
    {
        Battery::Absent
    }
}

#[cfg(target_os = "macos")]
type CFTypeRef = *const std::ffi::c_void;
#[cfg(target_os = "macos")]
type CFArrayRef = *const std::ffi::c_void;
#[cfg(target_os = "macos")]
type CFDictionaryRef = *const std::ffi::c_void;
#[cfg(target_os = "macos")]
type CFStringRef = *const std::ffi::c_void;

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFArrayGetCount(array: CFArrayRef) -> isize;
    fn CFArrayGetValueAtIndex(array: CFArrayRef, index: isize) -> CFTypeRef;
    fn CFBooleanGetValue(value: CFTypeRef) -> u8;
    fn CFDictionaryGetValue(dict: CFDictionaryRef, key: CFTypeRef) -> CFTypeRef;
    fn CFEqual(left: CFTypeRef, right: CFTypeRef) -> bool;
    fn CFNumberGetValue(number: CFTypeRef, number_type: i32, value: *mut std::ffi::c_void) -> bool;
    fn CFRelease(value: CFTypeRef);
    fn CFStringCreateWithCString(
        allocator: CFTypeRef,
        string: *const std::ffi::c_char,
        encoding: u32,
    ) -> CFStringRef;
}

#[cfg(target_os = "macos")]
#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOPSCopyPowerSourcesInfo() -> CFTypeRef;
    fn IOPSCopyPowerSourcesList(blob: CFTypeRef) -> CFArrayRef;
    fn IOPSGetPowerSourceDescription(blob: CFTypeRef, power_source: CFTypeRef) -> CFDictionaryRef;
    fn IOPSGetTimeRemainingEstimate() -> f64;
}

#[cfg(target_os = "macos")]
const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
#[cfg(target_os = "macos")]
const K_CF_NUMBER_SINT64_TYPE: i32 = 4;
#[cfg(target_os = "macos")]
const K_IOPS_TIME_REMAINING_UNKNOWN: f64 = -1.0;
#[cfg(target_os = "macos")]
const K_IOPS_TIME_REMAINING_UNLIMITED: f64 = -2.0;

#[cfg(target_os = "macos")]
fn cf_key(name: &std::ffi::CStr) -> CFStringRef {
    // SAFETY: CoreFoundation returns an owned string for a valid NUL-terminated UTF-8 key.
    unsafe { CFStringCreateWithCString(std::ptr::null(), name.as_ptr(), K_CF_STRING_ENCODING_UTF8) }
}

#[cfg(target_os = "macos")]
fn dictionary_number(dict: CFDictionaryRef, key: CFStringRef) -> Option<i64> {
    // SAFETY: `dict` and `key` originate from the live IOPowerSources snapshot.
    let value = unsafe { CFDictionaryGetValue(dict, key) };
    if value.is_null() {
        return None;
    }
    let mut number = 0_i64;
    // SAFETY: The destination is a valid i64 and CFNumberGetValue only writes to it.
    let ok = unsafe {
        CFNumberGetValue(
            value,
            K_CF_NUMBER_SINT64_TYPE,
            (&mut number as *mut i64).cast(),
        )
    };
    ok.then_some(number)
}

#[cfg(target_os = "macos")]
fn dictionary_bool(dict: CFDictionaryRef, key: CFStringRef) -> Option<bool> {
    // SAFETY: `dict` and `key` originate from the live IOPowerSources snapshot.
    let value = unsafe { CFDictionaryGetValue(dict, key) };
    (!value.is_null()).then(|| {
        // SAFETY: The documented IOPowerSources boolean keys are CFBoolean values.
        unsafe { CFBooleanGetValue(value) != 0 }
    })
}

#[cfg(target_os = "macos")]
fn dictionary_string_equals(
    dict: CFDictionaryRef,
    key: CFStringRef,
    expected: CFStringRef,
) -> bool {
    // SAFETY: `dict`, `key`, and `expected` are live CoreFoundation objects.
    let value = unsafe { CFDictionaryGetValue(dict, key) };
    !value.is_null() && unsafe { CFEqual(value, expected) }
}

#[cfg(target_os = "macos")]
fn get_snapshot_macos(include_runtime: bool) -> PowerSnapshot {
    let info = unsafe { IOPSCopyPowerSourcesInfo() };
    if info.is_null() {
        return PowerSnapshot {
            battery: Battery::Absent,
            runtime: RuntimeEstimate::Unavailable,
        };
    }

    let list = unsafe { IOPSCopyPowerSourcesList(info) };
    let type_key = cf_key(
        std::ffi::CString::new("Type")
            .expect("static power source key")
            .as_c_str(),
    );
    let present_key = cf_key(
        std::ffi::CString::new("Is Present")
            .expect("static power source key")
            .as_c_str(),
    );
    let internal_battery_type = cf_key(
        std::ffi::CString::new("InternalBattery")
            .expect("static power source type")
            .as_c_str(),
    );
    let battery = if list.is_null() || unsafe { CFArrayGetCount(list) } == 0 {
        Battery::Absent
    } else {
        let mut battery = Battery::Absent;
        for index in 0..unsafe { CFArrayGetCount(list) } {
            let source = unsafe { CFArrayGetValueAtIndex(list, index) };
            let dict = unsafe { IOPSGetPowerSourceDescription(info, source) };
            // 外部電源や未接続の電池を選ぶと、ノート型Macの実バッテリー表示が
            // 0%や誤った状態になるため、InternalBatteryかつ接続中の電源だけを採用する。
            if dict.is_null()
                || !dictionary_string_equals(dict, type_key, internal_battery_type)
                || !dictionary_bool(dict, present_key).unwrap_or(false)
            {
                continue;
            }
            battery = battery_from_dictionary(dict).unwrap_or(Battery::Absent);
            break;
        }
        battery
    };

    unsafe {
        CFRelease(type_key);
        CFRelease(present_key);
        CFRelease(internal_battery_type);
    }
    let result = PowerSnapshot {
        battery,
        runtime: runtime_for_snapshot(include_runtime),
    };

    if !list.is_null() {
        unsafe { CFRelease(list) };
    }
    unsafe { CFRelease(info) };
    result
}

#[cfg(target_os = "macos")]
fn runtime_for_snapshot(include_runtime: bool) -> RuntimeEstimate {
    if include_runtime {
        runtime_estimate()
    } else {
        // watch経路では既存のBattery契約だけが必要であり、runtime取得を省いて
        // 余計なI/Oと表示契約の変更を避ける。
        RuntimeEstimate::Unavailable
    }
}

#[cfg(target_os = "macos")]
fn battery_from_dictionary(dict: CFDictionaryRef) -> Option<Battery> {
    use std::ffi::CString;

    let current_key = cf_key(CString::new("Current Capacity").ok()?.as_c_str());
    let charging_key = cf_key(CString::new("Is Charging").ok()?.as_c_str());
    let charged_key = cf_key(CString::new("Is Charged").ok()?.as_c_str());
    let state_key = cf_key(CString::new("Power Source State").ok()?.as_c_str());
    let battery_state = cf_key(CString::new("Battery Power").ok()?.as_c_str());

    let result = dictionary_number(dict, current_key).and_then(|current| {
        let charging = dictionary_bool(dict, charging_key).unwrap_or(false);
        let charged = dictionary_bool(dict, charged_key).unwrap_or(false);
        let on_battery = dictionary_string_equals(dict, state_key, battery_state);
        battery_from_fields(current, charging, charged, on_battery)
    });

    unsafe {
        CFRelease(current_key);
        CFRelease(charging_key);
        CFRelease(charged_key);
        CFRelease(state_key);
        CFRelease(battery_state);
    }
    result
}

fn battery_from_fields(
    current_capacity: i64,
    charging: bool,
    charged: bool,
    on_battery: bool,
) -> Option<Battery> {
    let percent = u8::try_from(current_capacity).ok()?.min(100);
    let state = map_charging_state(charging, charged, on_battery);
    Some(Battery::Present { percent, state })
}

fn map_charging_state(charging: bool, charged: bool, on_battery: bool) -> ChargingState {
    if charging {
        ChargingState::Charging
    } else if charged || !on_battery {
        ChargingState::Charged
    } else {
        ChargingState::Discharging
    }
}

#[cfg(target_os = "macos")]
fn runtime_estimate() -> RuntimeEstimate {
    let seconds = unsafe { IOPSGetTimeRemainingEstimate() };
    runtime_from_seconds(seconds)
}

#[cfg(target_os = "macos")]
fn runtime_from_seconds(seconds: f64) -> RuntimeEstimate {
    if seconds == K_IOPS_TIME_REMAINING_UNKNOWN {
        RuntimeEstimate::Unknown
    } else if seconds == K_IOPS_TIME_REMAINING_UNLIMITED {
        RuntimeEstimate::Unlimited
    } else if !seconds.is_finite() || seconds < 0.0 {
        RuntimeEstimate::Unknown
    } else {
        RuntimeEstimate::Seconds(seconds.floor() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// macOS以外では、電源情報を取得できないことをUnavailableで表す。
    fn non_macos_fallback_is_unavailable() {
        #[cfg(not(target_os = "macos"))]
        assert_eq!(get_snapshot().runtime, RuntimeEstimate::Unavailable);
    }

    #[test]
    /// IOPowerSourcesのUnknown/Unlimitedを残り0秒へ潰さない。
    fn runtime_states_are_not_collapsed_to_zero() {
        assert_ne!(RuntimeEstimate::Unknown, RuntimeEstimate::Seconds(0));
        assert_ne!(RuntimeEstimate::Unlimited, RuntimeEstimate::Seconds(0));
    }

    #[test]
    /// pmsetのテキスト解析ではなく、電源源フィールドの組み合わせを状態へ変換する。
    fn maps_power_source_state_without_pmset_text() {
        assert_eq!(
            map_charging_state(true, false, true),
            ChargingState::Charging
        );
        assert_eq!(
            map_charging_state(false, false, true),
            ChargingState::Discharging
        );
        assert_eq!(
            map_charging_state(false, true, true),
            ChargingState::Charged
        );
        assert_eq!(
            map_charging_state(false, false, false),
            ChargingState::Charged
        );
    }

    #[test]
    /// IOPowerSourcesの生フィールドを既存のBattery表示契約へ変換する。
    fn converts_power_source_fields_to_existing_battery_contract() {
        assert_eq!(
            battery_from_fields(81, false, false, true),
            Some(Battery::Present {
                percent: 81,
                state: ChargingState::Discharging,
            })
        );
        assert_eq!(
            battery_from_fields(100, false, true, false),
            Some(Battery::Present {
                percent: 100,
                state: ChargingState::Charged,
            })
        );
        assert_eq!(battery_from_fields(-1, false, false, true), None);
    }

    #[test]
    #[cfg(target_os = "macos")]
    /// macOS固有の残り時間センチネル値と秒数をモデル化する。
    fn maps_iopower_sources_runtime_sentinels() {
        assert_eq!(runtime_from_seconds(-1.0), RuntimeEstimate::Unknown);
        assert_eq!(runtime_from_seconds(-2.0), RuntimeEstimate::Unlimited);
        assert_eq!(runtime_from_seconds(61.9), RuntimeEstimate::Seconds(61));
    }

    #[test]
    #[cfg(target_os = "macos")]
    /// 実機のIOPowerSources読み取りが全てのruntime状態を安全に返すことを確認する。
    fn iopower_sources_snapshot_is_safe_to_read() {
        let snapshot = get_snapshot();
        assert!(matches!(
            snapshot.runtime,
            RuntimeEstimate::Seconds(_)
                | RuntimeEstimate::Unknown
                | RuntimeEstimate::Unlimited
                | RuntimeEstimate::Unavailable
        ));
    }
}
