# Power / Thermal Research Probes

Research-only probes。production binaryから分離している。

## Environment

- Apple Silicon Mac
- macOS 27.0 / Darwin 27.0
- Swift 6.4

## Commands

```bash
clang -fobjc-arc -framework Foundation -framework IOKit \
  -o /tmp/iopowersources-probe iopowersources_probe.m
/tmp/iopowersources-probe

clang -fobjc-arc -framework Foundation \
  -o /tmp/thermalstate-probe thermalstate_probe.m
/tmp/thermalstate-probe
```

Swift source variants are also present, but this host's Swift compiler and SDK
minor versions do not match, so the Objective-C probes are the reproducible
execution path in this environment.

## Purpose

- `iopowersources_probe.swift`: Apple公開IOPowerSources APIからpower source fieldsとestimated runtimeを取得できるか確認する。
- `thermalstate_probe.swift`: Foundation `ProcessInfo.thermalState`を取得できるか確認する。

実験結果はTechnical Research文書へ転記する。これらはproduction implementationではない。

## Result (2026-09-21)

Objective-C probes compiled and ran successfully on the Apple Silicon Mac:

- IOPowerSources: one source, `InternalBattery-0`
- State: `Battery Power`
- Current / max capacity: `78 / 100`
- Charging: `0`
- Estimated runtime: `unlimited`
- `time_to_empty`: `793` on the first shared run; `783` on a rerun
- Thermal state: raw value `0`, `nominal`

The `time_to_empty` value changed by 10 seconds between runs, so it is a
dynamic estimate rather than a fixed battery property.

The Swift variants could not compile because the installed Swift compiler and
macOS SDK minor versions did not match. The Objective-C probes are the
reproducible execution path for this experiment.
