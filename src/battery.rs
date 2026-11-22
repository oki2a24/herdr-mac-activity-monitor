//! バッテリー状態の入出力層。macOS の IOPowerSources を呼び出す。
//!
//! ウィンドウタイトルで使う既存の `Battery` 型への変換と、ポップアップ専用の
//! 推定残り時間の取得を同じ情報から行う。pmsetによる代替取得は行わない。
use crate::parsers::{Battery, ChargingState};

#[derive(Debug, PartialEq, Eq)]
/// IOPowerSources が返す、バッテリー残り時間の状態を表す。
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
/// IOPowerSources から1回で取得した電源情報。
pub struct PowerSnapshot {
    /// 既存のウィンドウタイトル表示と共有するバッテリー状態。
    pub battery: Battery,
    /// ポップアップに表示する残り時間の状態。
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

/// 既存の監視経路向けにバッテリー状態を取得する。表示形式は変えない。
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
struct BatterySource {
    dictionary: CFDictionaryRef,
    is_internal: bool,
    is_present: bool,
}

#[cfg(target_os = "macos")]
fn first_present_internal_battery<I, F>(sources: Option<I>, mut read_battery: F) -> Battery
where
    I: IntoIterator<Item = Option<BatterySource>>,
    F: FnMut(CFDictionaryRef) -> Option<Battery>,
{
    let Some(sources) = sources else {
        return Battery::Absent;
    };

    for source in sources.into_iter().flatten() {
        if source.is_internal && source.is_present {
            return read_battery(source.dictionary).unwrap_or(Battery::Absent);
        }
    }
    Battery::Absent
}

#[cfg(target_os = "macos")]
fn with_cf_array_if_non_null<T, F>(array: CFArrayRef, read: F) -> Option<T>
where
    F: FnOnce(CFArrayRef) -> T,
{
    (!array.is_null()).then(|| read(array))
}

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
fn release_cf_object_if_non_null_with<F>(value: CFTypeRef, release: F)
where
    F: FnOnce(CFTypeRef),
{
    if !value.is_null() {
        release(value);
    }
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
    let sources = with_cf_array_if_non_null(list, |list| {
        let count = unsafe { CFArrayGetCount(list) };
        (0..count).map(move |index| {
            let source = unsafe { CFArrayGetValueAtIndex(list, index) };
            let dict = unsafe { IOPSGetPowerSourceDescription(info, source) };
            if dict.is_null() {
                return None;
            }
            Some(BatterySource {
                dictionary: dict,
                is_internal: dictionary_string_equals(dict, type_key, internal_battery_type),
                is_present: dictionary_bool(dict, present_key).unwrap_or(false),
            })
        })
    });
    // 外部電源や未接続の電池を選ぶと、ノート型Macの実バッテリー表示が
    // 0%や誤った状態になるため、InternalBatteryかつ接続中の電源だけを採用する。
    let battery = first_present_internal_battery(sources, battery_from_dictionary);

    unsafe {
        CFRelease(type_key);
        CFRelease(present_key);
        CFRelease(internal_battery_type);
    }
    let result = PowerSnapshot {
        battery,
        runtime: runtime_for_snapshot(include_runtime),
    };

    release_cf_object_if_non_null_with(list, |value| {
        // SAFETY: `value` はIOKitから所有権付きで返された電源一覧。
        unsafe { CFRelease(value) };
    });
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

    #[cfg(target_os = "macos")]
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFDataCreate(allocator: CFTypeRef, bytes: *const u8, length: isize) -> CFTypeRef;
        fn CFPropertyListCreateWithData(
            allocator: CFTypeRef,
            data: CFTypeRef,
            options: u64,
            format: *mut isize,
            error: *mut CFTypeRef,
        ) -> CFTypeRef;
    }

    #[cfg(target_os = "macos")]
    struct OwnedCfObject(CFTypeRef);

    #[cfg(target_os = "macos")]
    impl Drop for OwnedCfObject {
        fn drop(&mut self) {
            // SAFETY: このラッパーはCreate APIが返した所有権付きオブジェクトを保持する。
            unsafe { CFRelease(self.0) };
        }
    }

    #[cfg(target_os = "macos")]
    fn property_list_dictionary(entries: &str) -> OwnedCfObject {
        let xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
             \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
             <plist version=\"1.0\"><dict>{entries}</dict></plist>"
        );
        // SAFETY: `xml` は呼び出し中有効なバイト列で、CFDataCreateが内容をコピーする。
        let data = unsafe {
            CFDataCreate(
                std::ptr::null(),
                xml.as_ptr(),
                isize::try_from(xml.len()).expect("property list XML length fits CFIndex"),
            )
        };
        assert!(
            !data.is_null(),
            "CFDataCreate should accept valid XML bytes"
        );

        // SAFETY: `data` は有効なCFDataであり、他の引数は任意出力を要求しない。
        let property_list = unsafe {
            CFPropertyListCreateWithData(
                std::ptr::null(),
                data,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        // SAFETY: このテストがCFDataCreateから得た所有権をここで手放す。
        unsafe { CFRelease(data) };
        assert!(
            !property_list.is_null(),
            "CoreFoundation should parse the test property list"
        );
        OwnedCfObject(property_list)
    }

    #[test]
    /// macOS以外では、電源情報を取得できない状態として表す。
    // why: macOS以外で取得できない状態を、正常値やゼロと誤認しないため。
    fn non_macos_fallback_is_unavailable() {
        #[cfg(not(target_os = "macos"))]
        assert_eq!(get_snapshot().runtime, RuntimeEstimate::Unavailable);
    }

    #[test]
    /// IOPowerSourcesのUnknown/Unlimitedを残り0秒へ潰さない。
    // why: 推定不能と時間制限なしを、残り0秒と取り違えないため。
    fn runtime_states_are_not_collapsed_to_zero() {
        assert_ne!(RuntimeEstimate::Unknown, RuntimeEstimate::Seconds(0));
        assert_ne!(RuntimeEstimate::Unlimited, RuntimeEstimate::Seconds(0));
    }

    #[test]
    /// pmsetのテキスト解析に頼らず、電源情報の各項目から状態を決める。
    // why: 外部コマンドの文章に依存せず、電源情報から状態を決めるため。
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
    /// IOPowerSourcesの各項目を既存のバッテリー表示形式へ変換する。
    // why: 新しい取得元でも、既存のバッテリー表示契約を保つため。
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
    /// CoreFoundation辞書の容量と充電中フラグを既存の状態へ変換する。
    // why: CFNumberとCFBooleanの実値を読み、充電中の表示を維持するため。
    fn reads_charging_battery_from_core_foundation_dictionary() {
        let dictionary = property_list_dictionary(
            "<key>Current Capacity</key><integer>81</integer>\
             <key>Is Charging</key><true/>\
             <key>Is Charged</key><false/>\
             <key>Power Source State</key><string>Battery Power</string>",
        );

        assert_eq!(
            battery_from_dictionary(dictionary.0 as CFDictionaryRef),
            Some(Battery::Present {
                percent: 81,
                state: ChargingState::Charging,
            })
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    /// バッテリー駆動とAC電源の文字列を区別して状態へ変換する。
    // why: CFString比較の結果により放電中と充電完了を取り違えないため。
    fn distinguishes_battery_power_from_ac_power_in_dictionary() {
        let on_battery = property_list_dictionary(
            "<key>Current Capacity</key><integer>45</integer>\
             <key>Is Charging</key><false/>\
             <key>Is Charged</key><false/>\
             <key>Power Source State</key><string>Battery Power</string>",
        );
        let on_ac = property_list_dictionary(
            "<key>Current Capacity</key><integer>45</integer>\
             <key>Is Charging</key><false/>\
             <key>Is Charged</key><false/>\
             <key>Power Source State</key><string>AC Power</string>",
        );

        assert_eq!(
            battery_from_dictionary(on_battery.0 as CFDictionaryRef),
            Some(Battery::Present {
                percent: 45,
                state: ChargingState::Discharging,
            })
        );
        assert_eq!(
            battery_from_dictionary(on_ac.0 as CFDictionaryRef),
            Some(Battery::Present {
                percent: 45,
                state: ChargingState::Charged,
            })
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    /// 容量がない電源辞書を有効なバッテリー状態として扱わない。
    // why: 必須情報が欠けたOSデータを誤った割合で表示しないため。
    fn rejects_dictionary_without_current_capacity() {
        let dictionary = property_list_dictionary(
            "<key>Is Charging</key><true/>\
             <key>Power Source State</key><string>Battery Power</string>",
        );

        assert_eq!(
            battery_from_dictionary(dictionary.0 as CFDictionaryRef),
            None
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    /// macOS固有の残り時間を示す特別な値と秒数を状態へ変換する。
    // why: OSの特別な値を通常の残り時間と区別するため。
    fn maps_iopower_sources_runtime_sentinels() {
        assert_eq!(K_IOPS_TIME_REMAINING_UNKNOWN, -1.0);
        assert_eq!(K_IOPS_TIME_REMAINING_UNLIMITED, -2.0);
        assert_eq!(
            runtime_from_seconds(K_IOPS_TIME_REMAINING_UNKNOWN),
            RuntimeEstimate::Unknown
        );
        assert_eq!(
            runtime_from_seconds(K_IOPS_TIME_REMAINING_UNLIMITED),
            RuntimeEstimate::Unlimited
        );
        assert_eq!(runtime_from_seconds(61.9), RuntimeEstimate::Seconds(61));
    }

    #[test]
    #[cfg(target_os = "macos")]
    /// 特別値以外の負数と非有限値は推定不能として扱う。
    // why: 不正な時間を0秒や有効な残り時間として表示しないため。
    fn rejects_negative_and_non_finite_runtime_values() {
        assert_eq!(runtime_from_seconds(-3.0), RuntimeEstimate::Unknown);
        assert_eq!(runtime_from_seconds(f64::NAN), RuntimeEstimate::Unknown);
        assert_eq!(
            runtime_from_seconds(f64::INFINITY),
            RuntimeEstimate::Unknown
        );
        assert_eq!(
            runtime_from_seconds(f64::NEG_INFINITY),
            RuntimeEstimate::Unknown
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    /// 0秒は有効値として保持し、正の小数は秒単位で切り捨てる。
    // why: 0秒と不正な負数を区別し、表示する秒数の丸め方を固定するため。
    fn preserves_zero_and_floors_fractional_runtime_seconds() {
        assert_eq!(runtime_from_seconds(0.0), RuntimeEstimate::Seconds(0));
        assert_eq!(runtime_from_seconds(0.9), RuntimeEstimate::Seconds(0));
        assert_eq!(runtime_from_seconds(61.9), RuntimeEstimate::Seconds(61));
    }

    #[test]
    #[cfg(target_os = "macos")]
    /// 外部電源と未接続の内部電池を飛ばし、最初の有効な内部電池を選ぶ。
    // why: 電源一覧の先頭にある外部電源や未接続電池を誤表示しないため。
    fn selects_first_present_internal_battery_source() {
        let markers = [0_u8; 4];
        let pointer = |index: usize| &markers[index] as *const u8 as CFDictionaryRef;
        let selected = pointer(3);
        let candidates = Some(vec![
            None,
            Some(BatterySource {
                dictionary: pointer(0),
                is_internal: false,
                is_present: true,
            }),
            Some(BatterySource {
                dictionary: pointer(1),
                is_internal: true,
                is_present: false,
            }),
            Some(BatterySource {
                dictionary: selected,
                is_internal: true,
                is_present: true,
            }),
        ]);
        let mut read_dictionaries = Vec::new();

        let battery = first_present_internal_battery(candidates, |dictionary| {
            read_dictionaries.push(dictionary);
            Some(Battery::Present {
                percent: 81,
                state: ChargingState::Discharging,
            })
        });

        assert_eq!(
            battery,
            Battery::Present {
                percent: 81,
                state: ChargingState::Discharging,
            }
        );
        assert_eq!(read_dictionaries, vec![selected]);
    }

    #[test]
    #[cfg(target_os = "macos")]
    /// 電源一覧がない場合はAbsentを返し、辞書を読み取らない。
    // why: IOPowerSourcesがnilを返したときNULLを辿らず安全に終了するため。
    fn returns_absent_when_power_source_list_is_missing() {
        let mut read_called = false;

        let battery = first_present_internal_battery(None::<Vec<Option<BatterySource>>>, |_| {
            read_called = true;
            None
        });

        assert_eq!(battery, Battery::Absent);
        assert!(!read_called);
    }

    #[test]
    #[cfg(target_os = "macos")]
    /// NULLの配列は読み飛ばし、存在する配列だけ読み取り処理へ渡す。
    // why: NULL配列の参照によるクラッシュと、実データの読み飛ばしを防ぐため。
    fn reads_only_non_null_power_source_lists() {
        let marker = 0_u8;
        let array = &marker as *const u8 as CFArrayRef;
        let mut null_array_read = false;

        assert_eq!(with_cf_array_if_non_null(array, |read| read), Some(array));
        assert_eq!(
            with_cf_array_if_non_null(std::ptr::null(), |_| {
                null_array_read = true;
            }),
            None
        );
        assert!(!null_array_read);
    }

    #[test]
    #[cfg(target_os = "macos")]
    /// NULLでないCoreFoundationオブジェクトだけを解放処理へ渡す。
    // why: 電源一覧のリークを防ぎ、NULLを解放APIへ渡さないため。
    fn releases_only_non_null_core_foundation_objects() {
        let marker = 0_u8;
        let value = &marker as *const u8 as CFTypeRef;
        let mut released = Vec::new();

        release_cf_object_if_non_null_with(value, |released_value| {
            released.push(released_value);
        });
        release_cf_object_if_non_null_with(std::ptr::null(), |released_value| {
            released.push(released_value);
        });

        assert_eq!(released, vec![value]);
    }

    #[test]
    #[cfg(target_os = "macos")]
    /// 実機のIOPowerSources読み取りが全ての残り時間の状態を安全に返すことを確認する。
    // why: 実際のOS情報取得が安全に完了することを確かめるため。
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
