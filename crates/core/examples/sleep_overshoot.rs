//! How late `std::thread::sleep` returns on this machine, and the tick rate
//! `scheduler::next_tick` reaches with it, still and moving. The numbers behind
//! #1156 in `docs/research/performance-baseline-v1.md`.
//!
//! ```sh
//! cargo run --release -p fidget-core --example sleep_overshoot -- 2000
//! ```

use std::time::{Duration, Instant};

use fidget_core::scheduler::{next_tick, precise_sleep};

fn percentiles(label: &str, mut samples: Vec<f64>) {
    samples.sort_by(f64::total_cmp);
    let at = |p: f64| samples[((samples.len() - 1) as f64 * p).round() as usize];
    println!(
        "{label}: n={} p50={:.2} p90={:.2} p99={:.2} max={:.2} ms",
        samples.len(),
        at(0.50),
        at(0.90),
        at(0.99),
        samples[samples.len() - 1]
    );
}

fn sleeps(request: Duration, count: usize) {
    let returned = (0..count)
        .map(|_| {
            let start = Instant::now();
            std::thread::sleep(request);
            start.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    percentiles(&format!("sleep({request:?}) returned after"), returned);
}

fn tick_loop(tick: Duration, count: usize, moving: bool) {
    let first = Instant::now();
    let (mut deadline, mut woke) = (first, first);
    let mut gaps = Vec::with_capacity(count);
    for _ in 0..count {
        let now = Instant::now();
        deadline = next_tick(tick, deadline, woke, now, moving);
        precise_sleep(deadline - now, moving);
        let now = Instant::now();
        gaps.push(now.duration_since(woke).as_secs_f64() * 1000.0);
        woke = now;
    }
    let hz = count as f64 / first.elapsed().as_secs_f64();
    gaps.sort_by(f64::total_cmp);
    let at = |p: f64| gaps[((gaps.len() - 1) as f64 * p).round() as usize];
    let label = if moving { "moving" } else { "still" };
    println!(
        "{label} tick gap ({hz:.1} Hz): p50={:.2} p90={:.2} p99={:.2} max={:.2} ms",
        at(0.50),
        at(0.90),
        at(0.99),
        gaps[gaps.len() - 1]
    );
}

fn baseline_vs_precise(tick: Duration, count: usize) {
    println!("\n=== Baseline (sleep fallback) vs Precise (moving path) ===");
    let first = Instant::now();
    let (mut deadline, mut woke) = (first, first);
    let mut baseline_gaps = Vec::with_capacity(count);
    for _ in 0..count {
        let now = Instant::now();
        deadline = next_tick(tick, deadline, woke, now, false);
        precise_sleep(deadline - now, false);
        let now = Instant::now();
        baseline_gaps.push(now.duration_since(woke).as_secs_f64() * 1000.0);
        woke = now;
    }

    let first = Instant::now();
    let (mut deadline, mut woke) = (first, first);
    let mut precise_gaps = Vec::with_capacity(count);
    for _ in 0..count {
        let now = Instant::now();
        deadline = next_tick(tick, deadline, woke, now, true);
        precise_sleep(deadline - now, true);
        let now = Instant::now();
        precise_gaps.push(now.duration_since(woke).as_secs_f64() * 1000.0);
        woke = now;
    }

    baseline_gaps.sort_by(f64::total_cmp);
    precise_gaps.sort_by(f64::total_cmp);
    let at_baseline =
        |p: f64| baseline_gaps[((baseline_gaps.len() - 1) as f64 * p).round() as usize];
    let at_precise = |p: f64| precise_gaps[((precise_gaps.len() - 1) as f64 * p).round() as usize];
    println!(
        "baseline_sleep: n={} p50={:.2} p90={:.2} p99={:.2} max={:.2} ms",
        baseline_gaps.len(),
        at_baseline(0.50),
        at_baseline(0.90),
        at_baseline(0.99),
        baseline_gaps[baseline_gaps.len() - 1]
    );
    println!(
        "precise_moving: n={} p50={:.2} p90={:.2} p99={:.2} max={:.2} ms",
        precise_gaps.len(),
        at_precise(0.50),
        at_precise(0.90),
        at_precise(0.99),
        precise_gaps[precise_gaps.len() - 1]
    );
}

fn main() {
    let count = std::env::args()
        .nth(1)
        .and_then(|n| n.parse().ok())
        .unwrap_or(1000);
    let tick = Duration::from_millis(16);
    sleeps(tick, count);
    sleeps(Duration::from_millis(8), count);
    tick_loop(tick, count, false);
    tick_loop(tick, count, true);
    baseline_vs_precise(tick, count);
}
