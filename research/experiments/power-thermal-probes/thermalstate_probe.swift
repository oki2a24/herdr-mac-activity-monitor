import Foundation

let processInfo = ProcessInfo.processInfo
let state = processInfo.thermalState

print("thermal_state_raw_value=\(state.rawValue)")
switch state {
case .nominal:
    print("thermal_state=nominal")
case .fair:
    print("thermal_state=fair")
case .serious:
    print("thermal_state=serious")
case .critical:
    print("thermal_state=critical")
@unknown default:
    print("thermal_state=unknown")
}
