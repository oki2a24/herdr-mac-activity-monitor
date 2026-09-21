#import <Foundation/Foundation.h>

int main(void) {
    @autoreleasepool {
        NSProcessInfoThermalState state = NSProcessInfo.processInfo.thermalState;
        printf("thermal_state_raw_value=%ld\n", (long)state);
        switch (state) {
            case NSProcessInfoThermalStateNominal: puts("thermal_state=nominal"); break;
            case NSProcessInfoThermalStateFair: puts("thermal_state=fair"); break;
            case NSProcessInfoThermalStateSerious: puts("thermal_state=serious"); break;
            case NSProcessInfoThermalStateCritical: puts("thermal_state=critical"); break;
            default: puts("thermal_state=unknown"); break;
        }
    }
    return 0;
}
