#import <Foundation/Foundation.h>
#import <IOKit/ps/IOPowerSources.h>
#import <IOKit/ps/IOPSKeys.h>

static NSString *ValueString(NSDictionary *dictionary, NSString *key) {
    id value = dictionary[key];
    return value ? [value description] : @"n/a";
}

static NSString *KeyString(const char *key) {
    return [NSString stringWithUTF8String:key];
}

int main(void) {
    @autoreleasepool {
        CFTypeRef blob = IOPSCopyPowerSourcesInfo();
        CFArrayRef sources = IOPSCopyPowerSourcesList(blob);
        CFIndex count = CFArrayGetCount(sources);
        CFTimeInterval estimate = IOPSGetTimeRemainingEstimate();

        printf("source_count=%ld\n", (long)count);
        if (estimate == kIOPSTimeRemainingUnlimited) {
            puts("estimated_runtime=unlimited");
        } else if (estimate == kIOPSTimeRemainingUnknown) {
            puts("estimated_runtime=unknown");
        } else {
            printf("estimated_runtime_seconds=%.0f\n", estimate);
        }

        for (CFIndex index = 0; index < count; index++) {
            CFTypeRef source = CFArrayGetValueAtIndex(sources, index);
            CFDictionaryRef raw = IOPSGetPowerSourceDescription(blob, source);
            NSDictionary *description = (__bridge NSDictionary *)raw;
            printf("source[%ld].name=%s\n", (long)index, ValueString(description, KeyString(kIOPSNameKey)).UTF8String);
            printf("source[%ld].state=%s\n", (long)index, ValueString(description, KeyString(kIOPSPowerSourceStateKey)).UTF8String);
            printf("source[%ld].present=%s\n", (long)index, ValueString(description, KeyString(kIOPSIsPresentKey)).UTF8String);
            printf("source[%ld].current_capacity=%s\n", (long)index, ValueString(description, KeyString(kIOPSCurrentCapacityKey)).UTF8String);
            printf("source[%ld].max_capacity=%s\n", (long)index, ValueString(description, KeyString(kIOPSMaxCapacityKey)).UTF8String);
            printf("source[%ld].is_charging=%s\n", (long)index, ValueString(description, KeyString(kIOPSIsChargingKey)).UTF8String);
            printf("source[%ld].is_charged=%s\n", (long)index, ValueString(description, KeyString(kIOPSIsChargedKey)).UTF8String);
            printf("source[%ld].current=%s\n", (long)index, ValueString(description, KeyString(kIOPSCurrentKey)).UTF8String);
            printf("source[%ld].voltage=%s\n", (long)index, ValueString(description, KeyString(kIOPSVoltageKey)).UTF8String);
            printf("source[%ld].time_to_empty=%s\n", (long)index, ValueString(description, KeyString(kIOPSTimeToEmptyKey)).UTF8String);
        }

        CFRelease(sources);
        CFRelease(blob);
    }
    return 0;
}
