import Foundation
import IOKit
import IOKit.ps

func number(_ value: Any?) -> String {
    if let value = value as? NSNumber {
        return value.stringValue
    }
    return "n/a"
}
let blob = IOPSCopyPowerSourcesInfo().takeRetainedValue()
let sources = IOPSCopyPowerSourcesList(blob).takeRetainedValue() as [CFTypeRef]
let estimate = IOPSGetTimeRemainingEstimate()

print("source_count=\(sources.count)")
if estimate == kIOPSTimeRemainingUnlimited {
    print("estimated_runtime=unlimited")
} else if estimate == kIOPSTimeRemainingUnknown {
    print("estimated_runtime=unknown")
} else {
    print("estimated_runtime_seconds=\(estimate)")
}

for (index, source) in sources.enumerated() {
    guard let description = IOPSGetPowerSourceDescription(blob, source)?.takeUnretainedValue() as? [String: Any] else {
        print("source[\(index)]=unavailable")
        continue
    }

    print("source[\(index)].name=\(description[kIOPSNameKey as String] ?? "n/a")")
    print("source[\(index)].state=\(description[kIOPSPowerSourceStateKey as String] ?? "n/a")")
    print("source[\(index)].present=\(description[kIOPSIsPresentKey as String] ?? "n/a")")
    print("source[\(index)].current_capacity=\(number(description[kIOPSCurrentCapacityKey as String]))")
    print("source[\(index)].max_capacity=\(number(description[kIOPSMaxCapacityKey as String]))")
    print("source[\(index)].is_charging=\(description[kIOPSIsChargingKey as String] ?? "n/a")")
    print("source[\(index)].is_charged=\(description[kIOPSIsChargedKey as String] ?? "n/a")")
    print("source[\(index)].current=\(number(description[kIOPSCurrentKey as String]))")
    print("source[\(index)].voltage=\(number(description[kIOPSVoltageKey as String]))")
    print("source[\(index)].time_to_empty=\(number(description[kIOPSTimeToEmptyKey as String]))")
}
