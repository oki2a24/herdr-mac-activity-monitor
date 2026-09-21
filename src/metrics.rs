//! popup専用の詳細metric取得・状態表現・表示整形。

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread,
    time::{Duration, Instant, SystemTime},
};

use crate::{battery, parsers::MemInfo, processes::ProcessMemory};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// macOS のメモリプレッシャー通知を表示用の状態へ変換したもの。
pub enum MemoryPressure {
    /// 通知源を作成できない、またはまだ通知を受けていない状態。
    Unavailable,
    /// メモリプレッシャーが通常の状態。
    Normal,
    /// メモリプレッシャーが警告レベルの状態。
    Warn,
    /// メモリプレッシャーが危険レベルの状態。
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq)]
/// 2回のCPUサンプルから算出したCPU使用率。
pub enum CpuUtilization {
    /// 初回サンプル、取得失敗、またはスリープ復帰後で値を比較できない状態。
    Unavailable,
    /// CPU使用率。0.0から100.0の範囲に収めたパーセント値。
    Percent(f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// macOSが提供する熱状態。
pub enum ThermalState {
    /// 熱状態を取得できない状態。
    Unavailable,
    /// 通常状態。
    Nominal,
    /// 注意が必要な状態。
    Fair,
    /// 深刻な状態。
    Serious,
    /// 危険な状態。
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// swapの使用量と総容量（バイト単位）。
pub struct SwapUsage {
    /// 使用中のswap容量。
    pub used_bytes: u64,
    /// swapの総容量。
    pub total_bytes: u64,
}

#[derive(Debug)]
/// popupの1回分の表示データ。
pub struct PopupSnapshot {
    /// 既存のメモリ使用量。取得失敗時はNone。
    pub memory: Option<MemInfo>,
    /// 現在のメモリプレッシャー。
    pub pressure: MemoryPressure,
    /// swap使用量。取得失敗時はNone。
    pub swap: Option<SwapUsage>,
    /// physical footprintの大きい上位プロセス。
    pub processes: Vec<ProcessMemory>,
    /// CPU使用率。
    pub cpu: CpuUtilization,
    /// 熱状態。
    pub thermal: ThermalState,
    /// バッテリー状態。
    pub battery: crate::parsers::Battery,
    /// バッテリー残り時間。
    pub runtime: battery::RuntimeEstimate,
}

/// popup更新のために、前回との差分が必要なcollectorを保持する状態。
pub struct PopupState {
    pressure: MemoryPressureMonitor,
    cpu: CpuSampler,
    processes: ProcessScanner,
}

impl PopupState {
    /// 各collectorを初期化する。
    pub fn new() -> Self {
        Self {
            pressure: MemoryPressureMonitor::new(),
            cpu: CpuSampler::default(),
            processes: ProcessScanner::new(),
        }
    }

    /// 渡されたメモリ値とmacOSの詳細metricをまとめて1スナップショットにする。
    pub fn collect(&mut self, memory: Option<MemInfo>) -> PopupSnapshot {
        let power = battery::get_snapshot();
        PopupSnapshot {
            memory,
            pressure: self.pressure.state(),
            swap: get_swap_usage().ok(),
            processes: self.processes.collect(),
            cpu: self.cpu.sample(),
            thermal: get_thermal_state(),
            battery: power.battery,
            runtime: power.runtime,
        }
    }
}

struct ProcessScanner {
    request: Option<Sender<()>>,
    results: Receiver<Vec<ProcessMemory>>,
    cancel: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
    latest: Vec<ProcessMemory>,
    in_flight: bool,
}

impl ProcessScanner {
    fn new() -> Self {
        let (request, requests) = mpsc::channel();
        let (results, result_receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(true));
        let worker_cancel = Arc::clone(&cancel);
        // footprintはPIDごとの外部コマンド呼び出しで遅くなり得るため、
        // TUIの描画スレッドとは分離してpopupの入力応答を止めない。
        let worker = thread::spawn(move || {
            while requests.recv().is_ok() {
                if !worker_cancel.load(Ordering::Acquire) {
                    break;
                }
                let top = crate::processes::scan_top_three(Some(&worker_cancel));
                if results.send(top).is_err() {
                    break;
                }
            }
        });
        Self {
            request: Some(request),
            results: result_receiver,
            cancel,
            worker: Some(worker),
            latest: Vec::new(),
            in_flight: false,
        }
    }

    fn collect(&mut self) -> Vec<ProcessMemory> {
        let mut completed = false;
        while let Ok(result) = self.results.try_recv() {
            self.latest = result;
            self.in_flight = false;
            completed = true;
        }
        if !self.in_flight {
            if let Some(request) = &self.request {
                if request.send(()).is_ok() {
                    self.in_flight = true;
                }
            }
        }
        if completed {
            self.latest.clone()
        } else {
            // 実行中のscan結果を前回値として表示すると、現在のプロセス一覧と
            // 誤認されるため、完了するまでは明示的にunavailableとする。
            Vec::new()
        }
    }
}

impl Drop for ProcessScanner {
    fn drop(&mut self) {
        self.cancel.store(false, Ordering::Release);
        self.request.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Default for PopupState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy)]
struct CpuTicks {
    busy: u64,
    total: u64,
}

#[derive(Debug, Clone, Copy)]
struct CpuSample {
    ticks: CpuTicks,
    at: Instant,
    wall_at: SystemTime,
}

#[derive(Default)]
struct CpuSampler {
    previous: Option<CpuSample>,
}

impl CpuSampler {
    fn sample(&mut self) -> CpuUtilization {
        let now = Instant::now();
        let wall_now = SystemTime::now();
        let current = read_cpu_ticks().ok();
        let previous = self.previous;
        self.previous = current.map(|ticks| CpuSample {
            ticks,
            at: now,
            wall_at: wall_now,
        });

        let Some(current) = current else {
            return CpuUtilization::Unavailable;
        };
        let Some(previous) = previous else {
            return CpuUtilization::Unavailable;
        };
        let wall_elapsed = wall_now.duration_since(previous.wall_at).ok();
        // Instantだけではスリープ復帰後の経過を表現できる環境差があるため、
        // wall clockも併用し、長い中断をCPU使用率として誤表示しない。
        if now.duration_since(previous.at) > Duration::from_secs(5)
            || wall_elapsed.is_none_or(|elapsed| elapsed > Duration::from_secs(5))
        {
            return CpuUtilization::Unavailable;
        }
        calculate_cpu_utilization(previous.ticks, current)
            .map(CpuUtilization::Percent)
            .unwrap_or(CpuUtilization::Unavailable)
    }
}

fn calculate_cpu_utilization(previous: CpuTicks, current: CpuTicks) -> Option<f64> {
    let total_delta = current.total.checked_sub(previous.total)?;
    let busy_delta = current.busy.checked_sub(previous.busy)?;
    if total_delta == 0 || busy_delta > total_delta {
        return None;
    }
    Some((busy_delta as f64 / total_delta as f64 * 100.0).clamp(0.0, 100.0))
}

#[cfg(target_os = "macos")]
type KernReturn = i32;
#[cfg(target_os = "macos")]
type MachPort = u32;
#[cfg(target_os = "macos")]
type Natural = u32;
#[cfg(target_os = "macos")]
type Integer = i32;
#[cfg(target_os = "macos")]
type ProcessorInfoArray = *mut Integer;

#[cfg(target_os = "macos")]
const PROCESSOR_CPU_LOAD_INFO: i32 = 2;
#[cfg(target_os = "macos")]
const CPU_STATE_USER: usize = 0;
#[cfg(target_os = "macos")]
const CPU_STATE_SYSTEM: usize = 1;
#[cfg(target_os = "macos")]
const CPU_STATE_IDLE: usize = 2;
#[cfg(target_os = "macos")]
const CPU_STATE_NICE: usize = 3;
#[cfg(target_os = "macos")]
const CPU_STATE_MAX: usize = 4;

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn mach_host_self() -> MachPort;
    fn host_processor_info(
        host: MachPort,
        flavor: i32,
        processor_count: *mut Natural,
        processor_info: *mut ProcessorInfoArray,
        processor_info_count: *mut Natural,
    ) -> KernReturn;
    fn mach_task_self() -> MachPort;
    fn vm_deallocate(task: MachPort, address: usize, size: usize) -> KernReturn;
}

#[cfg(target_os = "macos")]
fn read_cpu_ticks() -> Result<CpuTicks, String> {
    let mut processor_count = 0;
    let mut info: ProcessorInfoArray = std::ptr::null_mut();
    let mut info_count = 0;
    // SAFETY: All pointers refer to writable local variables, and the Mach API owns the returned
    // info buffer until it is released below.
    let result = unsafe {
        host_processor_info(
            mach_host_self(),
            PROCESSOR_CPU_LOAD_INFO,
            &mut processor_count,
            &mut info,
            &mut info_count,
        )
    };
    if result != 0 || info.is_null() {
        return Err(format!("host_processor_info failed: {result}"));
    }

    let values = unsafe { std::slice::from_raw_parts(info, info_count as usize) };
    let mut user = 0_u64;
    let mut system = 0_u64;
    let mut idle = 0_u64;
    let mut nice = 0_u64;
    for cpu in values
        .chunks_exact(CPU_STATE_MAX)
        .take(processor_count as usize)
    {
        user = user.saturating_add(u64::try_from(cpu[CPU_STATE_USER]).unwrap_or(0));
        system = system.saturating_add(u64::try_from(cpu[CPU_STATE_SYSTEM]).unwrap_or(0));
        idle = idle.saturating_add(u64::try_from(cpu[CPU_STATE_IDLE]).unwrap_or(0));
        nice = nice.saturating_add(u64::try_from(cpu[CPU_STATE_NICE]).unwrap_or(0));
    }
    // SAFETY: This is the address and exact byte length returned by host_processor_info.
    // 返却バッファを解放しないと、popup更新のたびにMachメモリがリークするため、
    // 集計直後に所有権を返す。
    unsafe {
        vm_deallocate(
            mach_task_self(),
            info.cast::<u8>() as usize,
            info_count as usize * std::mem::size_of::<Integer>(),
        );
    }
    let busy = user.saturating_add(system).saturating_add(nice);
    Ok(CpuTicks {
        busy,
        total: busy.saturating_add(idle),
    })
}

#[cfg(not(target_os = "macos"))]
fn read_cpu_ticks() -> Result<CpuTicks, String> {
    Err("CPU utilization is only available on macOS".to_string())
}

#[repr(C)]
#[cfg(target_os = "macos")]
struct XswUsage {
    total: u64,
    available: u64,
    used: u64,
    page_size: u32,
    encrypted: u32,
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn sysctlbyname(
        name: *const std::ffi::c_char,
        oldp: *mut std::ffi::c_void,
        oldlenp: *mut usize,
        newp: *mut std::ffi::c_void,
        newlen: usize,
    ) -> i32;
}

#[cfg(target_os = "macos")]
fn get_swap_usage() -> Result<SwapUsage, String> {
    let name = std::ffi::CString::new("vm.swapusage").map_err(|e| e.to_string())?;
    let mut usage = XswUsage {
        total: 0,
        available: 0,
        used: 0,
        page_size: 0,
        encrypted: 0,
    };
    let mut length = std::mem::size_of::<XswUsage>();
    // SAFETY: `usage` and `length` are valid writable buffers of the declared size.
    let result = unsafe {
        sysctlbyname(
            name.as_ptr(),
            (&mut usage as *mut XswUsage).cast(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    if result != 0 || length < std::mem::size_of::<XswUsage>() {
        return Err("sysctl vm.swapusage failed".to_string());
    }
    Ok(SwapUsage {
        used_bytes: usage.used,
        total_bytes: usage.total,
    })
}

#[cfg(not(target_os = "macos"))]
fn get_swap_usage() -> Result<SwapUsage, String> {
    Err("swap usage is only available on macOS".to_string())
}

#[cfg(target_os = "macos")]
#[link(name = "Foundation", kind = "framework")]
unsafe extern "C" {}

#[cfg(target_os = "macos")]
#[link(name = "objc")]
unsafe extern "C" {
    fn objc_getClass(name: *const std::ffi::c_char) -> *mut std::ffi::c_void;
    fn sel_registerName(name: *const std::ffi::c_char) -> *mut std::ffi::c_void;
    fn objc_msgSend();
}

#[cfg(target_os = "macos")]
fn get_thermal_state() -> ThermalState {
    use std::ffi::CString;

    let Ok(class_name) = CString::new("NSProcessInfo") else {
        return ThermalState::Unavailable;
    };
    let Ok(process_info_name) = CString::new("processInfo") else {
        return ThermalState::Unavailable;
    };
    let Ok(thermal_name) = CString::new("thermalState") else {
        return ThermalState::Unavailable;
    };
    // SAFETY: The Objective-C class and selectors are static Foundation API names. The
    // transmuted signatures match `+[NSProcessInfo processInfo]` and
    // `-[NSProcessInfo thermalState]`.
    let class = unsafe { objc_getClass(class_name.as_ptr()) };
    if class.is_null() {
        return ThermalState::Unavailable;
    }
    let info = unsafe {
        let send: unsafe extern "C" fn(
            *mut std::ffi::c_void,
            *mut std::ffi::c_void,
        ) -> *mut std::ffi::c_void = std::mem::transmute(objc_msgSend as *const ());
        send(class, sel_registerName(process_info_name.as_ptr()))
    };
    if info.is_null() {
        return ThermalState::Unavailable;
    }
    let state = unsafe {
        let send: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> isize =
            std::mem::transmute(objc_msgSend as *const ());
        send(info, sel_registerName(thermal_name.as_ptr()))
    };
    match state {
        0 => ThermalState::Nominal,
        1 => ThermalState::Fair,
        2 => ThermalState::Serious,
        3 => ThermalState::Critical,
        _ => ThermalState::Unavailable,
    }
}

#[cfg(not(target_os = "macos"))]
fn get_thermal_state() -> ThermalState {
    ThermalState::Unavailable
}

#[cfg(target_os = "macos")]
const DISPATCH_MEMORYPRESSURE_NORMAL: usize = 0x1;
#[cfg(target_os = "macos")]
const DISPATCH_MEMORYPRESSURE_WARN: usize = 0x2;
#[cfg(target_os = "macos")]
const DISPATCH_MEMORYPRESSURE_CRITICAL: usize = 0x4;

#[cfg(target_os = "macos")]
type DispatchSource = *mut std::ffi::c_void;

#[cfg(target_os = "macos")]
#[repr(C)]
struct DispatchSourceType {
    _private: [u8; 0],
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    static _dispatch_source_type_memorypressure: DispatchSourceType;
    fn dispatch_get_global_queue(identifier: isize, flags: usize) -> *mut std::ffi::c_void;
    fn dispatch_source_create(
        kind: *const std::ffi::c_void,
        handle: usize,
        mask: usize,
        queue: *mut std::ffi::c_void,
    ) -> DispatchSource;
    fn dispatch_set_context(object: DispatchSource, context: *mut std::ffi::c_void);
    fn dispatch_source_set_event_handler_f(
        source: DispatchSource,
        handler: Option<unsafe extern "C" fn(*mut std::ffi::c_void)>,
    );
    fn dispatch_source_set_cancel_handler_f(
        source: DispatchSource,
        handler: Option<unsafe extern "C" fn(*mut std::ffi::c_void)>,
    );
    fn dispatch_source_get_data(source: DispatchSource) -> usize;
    fn dispatch_resume(object: DispatchSource);
    fn dispatch_source_cancel(source: DispatchSource);
    fn dispatch_release(object: DispatchSource);
}

#[cfg(target_os = "macos")]
struct PressureContext {
    source: std::sync::atomic::AtomicPtr<std::ffi::c_void>,
    state: std::sync::atomic::AtomicU8,
}

struct MemoryPressureMonitor {
    #[cfg(target_os = "macos")]
    source: DispatchSource,
    #[cfg(target_os = "macos")]
    context: *mut PressureContext,
}

impl MemoryPressureMonitor {
    fn new() -> Self {
        #[cfg(target_os = "macos")]
        {
            // SAFETY: The dispatch source type and queue are provided by libdispatch.
            let source = unsafe {
                dispatch_source_create(
                    (&_dispatch_source_type_memorypressure as *const DispatchSourceType).cast(),
                    0,
                    DISPATCH_MEMORYPRESSURE_NORMAL
                        | DISPATCH_MEMORYPRESSURE_WARN
                        | DISPATCH_MEMORYPRESSURE_CRITICAL,
                    dispatch_get_global_queue(0, 0),
                )
            };
            if !source.is_null() {
                let context = Box::into_raw(Box::new(PressureContext {
                    source: std::sync::atomic::AtomicPtr::new(source),
                    state: std::sync::atomic::AtomicU8::new(0),
                }));
                // SAFETY: `context` remains owned by this monitor until the source is cancelled.
                unsafe {
                    dispatch_set_context(source, context.cast());
                    dispatch_source_set_event_handler_f(source, Some(memory_pressure_handler));
                    dispatch_source_set_cancel_handler_f(
                        source,
                        Some(memory_pressure_cancel_handler),
                    );
                    dispatch_resume(source);
                }
                return Self { source, context };
            }
        }
        // macOS以外、または通知源作成に失敗した場合は、collector全体を止めずに
        // この項目だけUnavailableとして表示する。
        Self {
            #[cfg(target_os = "macos")]
            source: std::ptr::null_mut(),
            #[cfg(target_os = "macos")]
            context: std::ptr::null_mut(),
        }
    }

    fn state(&self) -> MemoryPressure {
        #[cfg(target_os = "macos")]
        if !self.context.is_null() {
            // SAFETY: context is owned by self and remains valid while the source is active.
            let value = unsafe {
                (*self.context)
                    .state
                    .load(std::sync::atomic::Ordering::Acquire)
            };
            return match value {
                1 => MemoryPressure::Normal,
                2 => MemoryPressure::Warn,
                3 => MemoryPressure::Critical,
                _ => MemoryPressure::Unavailable,
            };
        }
        MemoryPressure::Unavailable
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn memory_pressure_handler(context: *mut std::ffi::c_void) {
    if context.is_null() {
        return;
    }
    // SAFETY: The context pointer was installed by MemoryPressureMonitor::new.
    let context = unsafe { &*(context.cast::<PressureContext>()) };
    let source = context.source.load(std::sync::atomic::Ordering::Acquire);
    if source.is_null() {
        return;
    }
    // SAFETY: source is the dispatch source that invoked this handler.
    let data = unsafe { dispatch_source_get_data(source) };
    let state = if data & DISPATCH_MEMORYPRESSURE_CRITICAL != 0 {
        3
    } else if data & DISPATCH_MEMORYPRESSURE_WARN != 0 {
        2
    } else if data & DISPATCH_MEMORYPRESSURE_NORMAL != 0 {
        1
    } else {
        0
    };
    context
        .state
        .store(state, std::sync::atomic::Ordering::Release);
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn memory_pressure_cancel_handler(context: *mut std::ffi::c_void) {
    if !context.is_null() {
        // SAFETY: libdispatch invokes this handler after all event handlers have completed, so
        // reclaiming the context here cannot race with memory_pressure_handler.
        unsafe { drop(Box::from_raw(context.cast::<PressureContext>())) };
    }
}

#[cfg(target_os = "macos")]
impl Drop for MemoryPressureMonitor {
    fn drop(&mut self) {
        if self.source.is_null() {
            return;
        }
        // SAFETY: The cancellation handler reclaims the context after outstanding event handlers
        // complete. Releasing the source here only drops the dispatch object itself.
        unsafe {
            dispatch_source_cancel(self.source);
            dispatch_release(self.source);
        }
    }
}

#[cfg(not(target_os = "macos"))]
impl Drop for MemoryPressureMonitor {
    fn drop(&mut self) {}
}

fn format_memory_pressure(value: MemoryPressure) -> &'static str {
    match value {
        MemoryPressure::Unavailable => "unavailable",
        MemoryPressure::Normal => "normal",
        MemoryPressure::Warn => "warn",
        MemoryPressure::Critical => "critical",
    }
}

fn format_cpu(value: CpuUtilization) -> String {
    match value {
        CpuUtilization::Unavailable => "unavailable".to_string(),
        CpuUtilization::Percent(value) => format!("{value:.1}%"),
    }
}

fn format_thermal(value: ThermalState) -> &'static str {
    match value {
        ThermalState::Unavailable => "unavailable",
        ThermalState::Nominal => "nominal",
        ThermalState::Fair => "fair",
        ThermalState::Serious => "serious",
        ThermalState::Critical => "critical",
    }
}

fn format_runtime(value: &battery::RuntimeEstimate) -> String {
    match value {
        battery::RuntimeEstimate::Seconds(seconds) => {
            let hours = seconds / 3600;
            let minutes = seconds % 3600 / 60;
            if hours > 0 {
                format!("{hours}h {minutes:02}m")
            } else {
                format!("{minutes}m")
            }
        }
        battery::RuntimeEstimate::Unknown => "unknown".to_string(),
        battery::RuntimeEstimate::Unlimited => "unlimited".to_string(),
        battery::RuntimeEstimate::Unavailable => "unavailable".to_string(),
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.2} GB", bytes as f64 / 1e9)
    } else {
        format!("{:.0} MB", bytes as f64 / 1e6)
    }
}

fn truncate_name(name: &str, max_chars: usize) -> String {
    let mut chars = name.chars();
    let result: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{result}…")
    } else {
        result
    }
}

/// popupの表示行を固定順で生成する。
pub fn format_popup_lines(snapshot: &PopupSnapshot) -> Vec<String> {
    let existing = crate::parsers::format_lines(&snapshot.memory, &snapshot.battery);
    let mut lines = vec![existing[0].clone(), existing[1].clone()];
    lines.push(format!(
        "Memory Pressure  {}",
        format_memory_pressure(snapshot.pressure)
    ));
    match snapshot.swap {
        Some(swap) => lines.push(format!(
            "Swap             {} / {}",
            format_bytes(swap.used_bytes),
            format_bytes(swap.total_bytes)
        )),
        None => lines.push("Swap             unavailable".to_string()),
    }
    lines.push(String::new());
    lines.push("Top 3 Available Processes (physical footprint)".to_string());
    if snapshot.processes.is_empty() {
        lines.push("  unavailable".to_string());
    } else {
        for process in &snapshot.processes {
            lines.push(format!(
                "  {:<32} PID {:>6} {}",
                truncate_name(&process.name, 32),
                process.pid,
                format_bytes(process.bytes)
            ));
        }
    }
    lines.push(String::new());
    lines.push(format!("CPU              {}", format_cpu(snapshot.cpu)));
    lines.push(format!(
        "Thermal          {}",
        format_thermal(snapshot.thermal)
    ));
    lines.push(String::new());
    lines.push(existing[2].clone());
    lines.push(format!(
        "Battery runtime  {}",
        format_runtime(&snapshot.runtime)
    ));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsers::{Battery, ChargingState, MemInfo};

    fn snapshot() -> PopupSnapshot {
        PopupSnapshot {
            memory: Some(MemInfo {
                used_gb: 8.0,
                total_gb: 16.0,
                percent: 50,
            }),
            pressure: MemoryPressure::Critical,
            swap: Some(SwapUsage {
                used_bytes: 1_000_000_000,
                total_bytes: 2_000_000_000,
            }),
            processes: vec![ProcessMemory {
                pid: 42,
                name: "local-llm".to_string(),
                bytes: 3_000_000_000,
            }],
            cpu: CpuUtilization::Percent(12.34),
            thermal: ThermalState::Serious,
            battery: Battery::Present {
                percent: 80,
                state: ChargingState::Discharging,
            },
            runtime: battery::RuntimeEstimate::Seconds(3_750),
        }
    }

    #[test]
    /// CPUカウンタが同値、または逆行した場合は有効な使用率を返さない。
    fn cpu_delta_is_unavailable_for_invalid_or_empty_delta() {
        let ticks = CpuTicks {
            busy: 10,
            total: 20,
        };
        assert_eq!(calculate_cpu_utilization(ticks, ticks), None);
        assert_eq!(
            calculate_cpu_utilization(ticks, CpuTicks { busy: 9, total: 30 }),
            None
        );
    }

    #[test]
    /// busy deltaをtotal deltaで割った値をCPU使用率として返す。
    fn cpu_delta_returns_busy_percentage() {
        assert_eq!(
            calculate_cpu_utilization(
                CpuTicks {
                    busy: 20,
                    total: 100
                },
                CpuTicks {
                    busy: 50,
                    total: 200
                },
            ),
            Some(30.0)
        );
    }

    #[test]
    /// popupが固定セクション順と各metricの明示的な表示値を持つ。
    fn popup_has_fixed_sections_and_explicit_states() {
        let lines = format_popup_lines(&snapshot());
        assert_eq!(lines[0], "Activity Monitor");
        assert_eq!(lines[1], "Memory   8.00 GB / 16.00 GB 50%");
        assert!(lines.iter().any(|line| line == "Memory Pressure  critical"));
        assert!(lines.iter().any(|line| line == "CPU              12.3%"));
        assert!(lines.iter().any(|line| line == "Thermal          serious"));
        assert!(lines.iter().any(|line| line == "Battery runtime  1h 02m"));
        assert!(lines.iter().any(|line| line.contains("PID     42")));
    }

    #[test]
    /// runtime取得失敗を0分と誤表示せず、Unavailableのまま表示する。
    fn popup_does_not_turn_unavailable_runtime_into_zero() {
        let mut value = snapshot();
        value.runtime = battery::RuntimeEstimate::Unavailable;
        let lines = format_popup_lines(&value);
        assert!(lines
            .iter()
            .any(|line| line == "Battery runtime  unavailable"));
        assert!(!lines.iter().any(|line| line == "Battery runtime  0m"));
    }

    #[test]
    /// 長すぎるプロセス名を表示幅内へ切り詰める。
    fn long_process_names_are_truncated() {
        let mut value = snapshot();
        value.processes[0].name = "a".repeat(64);
        let lines = format_popup_lines(&value);
        assert!(lines
            .iter()
            .any(|line| line.contains("a".repeat(32).as_str())));
        assert!(!lines.iter().any(|line| line.contains(&"a".repeat(64))));
    }

    #[test]
    #[cfg(target_os = "macos")]
    /// macOS固有collectorを実機上で呼び出してもpanicしない。
    fn macos_collectors_are_callable_without_panicking() {
        let _ = read_cpu_ticks();
        let _ = get_swap_usage();
        let _ = get_thermal_state();
        let pressure = MemoryPressureMonitor::new();
        assert_eq!(pressure.state(), MemoryPressure::Unavailable);
    }
}
