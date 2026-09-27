// What one window-list poll costs, measured outside fidget against whatever
// is on the desktop right now. Opens nothing and moves nothing, so it needs no
// green light. Each row times CGWindowListCopyWindowInfo plus the per-entry
// decode src-tauri/src/platform/macos/window_source.rs does (bounds, number,
// layer, and the owner name once Screen Recording consent is usable).
//
// Rows: `app-call` is the bare call with fidget's options (OnScreenOnly |
// ExcludeDesktopElements), `app` adds the decode fidget does without names,
// `app-names` is that call on the consent-on path, `all` swaps in the
// every-window option, which lists every Space and is the one free way to get
// a second window count out of the same desktop.
// Usage: swift scripts/bench-window-list-macos.swift [--iterations N] [--warmup N]

import CoreGraphics
import Foundation

var iterations = 200
var warmup = 20
var args = Array(CommandLine.arguments.dropFirst())
while !args.isEmpty {
    let flag = args.removeFirst()
    let value = args.isEmpty ? nil : Int(args.removeFirst())
    switch (flag, value) {
    case ("--iterations", let n?): iterations = n
    case ("--warmup", let n?): warmup = n
    default:
        FileHandle.standardError.write(
            Data("usage: bench-window-list-macos.swift [--iterations N] [--warmup N]\n".utf8))
        exit(2)
    }
}

let appOptions: CGWindowListOption = [.optionOnScreenOnly, .excludeDesktopElements]
let allOptions: CGWindowListOption = [.optionAll, .excludeDesktopElements]  // codespell:ignore

/// One poll as the app does it: the call, then every entry decoded. Returns
/// how many entries decoded, which is what `visible_windows` hands back.
func poll(_ options: CGWindowListOption, names: Bool, decode: Bool = true) -> Int {
    guard let list = CGWindowListCopyWindowInfo(options, kCGNullWindowID) else { return 0 }
    if !decode { return CFArrayGetCount(list) }
    var decoded = 0
    for entry in list as NSArray {
        guard let entry = entry as? NSDictionary,
            let bounds = entry[kCGWindowBounds] as? NSDictionary
        else { continue }
        var rect = CGRect.zero
        guard CGRectMakeWithDictionaryRepresentation(bounds as CFDictionary, &rect) else {
            continue
        }
        guard entry[kCGWindowNumber] as? NSNumber != nil,
            entry[kCGWindowLayer] as? NSNumber != nil
        else { continue }
        if names {
            guard entry[kCGWindowOwnerName] as? NSString != nil else { continue }
            _ = entry[kCGWindowName] as? NSString
        }
        decoded += 1
    }
    return decoded
}

func now() -> UInt64 { clock_gettime_nsec_np(CLOCK_UPTIME_RAW) }

func row(_ name: String, _ options: CGWindowListOption, names: Bool, decode: Bool = true) {
    for _ in 0..<warmup { _ = poll(options, names: names, decode: decode) }
    var samples: [UInt64] = []
    samples.reserveCapacity(iterations)
    var windows = 0
    for _ in 0..<iterations {
        let start = now()
        windows = poll(options, names: names, decode: decode)
        samples.append(now() - start)
    }
    samples.sort()
    let median = Double(samples[samples.count / 2]) / 1000
    let p95 = Double(samples[min(samples.count - 1, samples.count * 95 / 100)]) / 1000
    let max = Double(samples[samples.count - 1]) / 1000
    let perWindow = windows > 0 ? median / Double(windows) : 0
    print(
        String(
            format: "%@\t%d\t%d\t%.1f\t%.1f\t%.1f\t%.2f", name, windows, iterations, median, p95,
            max, perWindow))
}

print("mode\twindows\titerations\tmedian_us\tp95_us\tmax_us\tus_per_window")
row("app-call", appOptions, names: false, decode: false)
row("app", appOptions, names: false)
row("app-names", appOptions, names: true)
row("all", allOptions, names: false)
