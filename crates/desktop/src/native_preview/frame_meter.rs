//! Symmetric diagnostic generation/content witnesses; no presentation claim.
use crate::terminal::emulator::Listener;
use alacritty_terminal::{
    grid::Dimensions,
    index::{Column, Line, Point},
    term::Term,
    vte::ansi::Color,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
#[derive(Default)]
pub struct Generation {
    pub value: AtomicU64,
    pub ready: AtomicU64,
}
impl Generation {
    pub fn advance(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
        self.ready
            .store(crate::system::probe_mach_ticks(), Ordering::Relaxed);
    }
}
type Witness = (usize, u64, u64, u64, Vec<u32>);
static RECORDS: Mutex<Vec<Witness>> = Mutex::new(Vec::new());
static LOST: AtomicU64 = AtomicU64::new(0);
pub fn capture(term: &Term<Listener>, generation: &Arc<Generation>) {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if !*ENABLED.get_or_init(|| std::env::var_os("GUILD_PROBE_EMISSIONS").is_some()) {
        return;
    }
    if term.columns() < 52 || term.screen_lines() < 3 {
        return;
    }
    let mut sequences = Vec::with_capacity(2);
    for row in [0, 2] {
        let mut value = 0u64;
        let mut valid = true;
        for col in 2..50 {
            let bg = term.grid()[Point::new(Line(row), Column(col))].bg;
            let bit = match bg {
                Color::Spec(rgb) if rgb.g > 180 && rgb.b < 80 => 1,
                Color::Spec(rgb) if rgb.b > 180 && rgb.g < 80 => 0,
                _ => {
                    valid = false;
                    break;
                }
            };
            value = (value << 1) | bit;
        }
        if valid {
            sequences.push((value >> 16) as u32);
        }
    }
    let mut records = RECORDS.lock().unwrap();
    if records.len() < 100000 {
        records.push((
            Arc::as_ptr(generation) as usize,
            generation.value.load(Ordering::Relaxed),
            generation.ready.load(Ordering::Relaxed),
            crate::system::probe_mach_ticks(),
            sequences,
        ));
    } else {
        LOST.fetch_add(1, Ordering::Relaxed);
    }
}
static INPUTS: Mutex<Vec<(u64, u64)>> = Mutex::new(Vec::new());
pub fn input(sequence: u64) {
    INPUTS
        .lock()
        .unwrap()
        .push((sequence, crate::system::probe_mach_ticks()));
}
pub fn flush() {
    for (at, wait) in DEMAND_TIMES.lock().unwrap().drain(..) {
        println!(
            "{}",
            serde_json::json!({"demand_mach_ticks":at,"wait_ns":wait})
        );
    }
    for (sequence, mach_ticks) in INPUTS.lock().unwrap().drain(..) {
        println!(
            "{}",
            serde_json::json!({"input_dispatch":sequence,"mach_ticks":mach_ticks})
        );
    }
    for (terminal, generation, ready, paint, sequences) in RECORDS.lock().unwrap().drain(..) {
        println!(
            "{}",
            serde_json::json!({"grid_terminal":terminal,"grid_generation":generation,"ready_mach_ticks":ready,"paint_mach_ticks":paint,"sequences":sequences})
        );
    }
    println!(
        "{}",
        serde_json::json!({"grid_record_overflow":LOST.load(Ordering::Relaxed)})
    );
}

// Opt-in causal timing only; never a presentation measurement or an A/B sample.
static DEMAND_TIMES: Mutex<Vec<(u64, Option<u64>)>> = Mutex::new(Vec::new());
pub fn demand_action(wait_ns: Option<u64>) {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ENABLED.get_or_init(|| std::env::var_os("GUILD_PROBE_TIMING").is_some()) {
        let mut records = DEMAND_TIMES.lock().unwrap();
        if records.len() < 100000 {
            records.push((crate::system::probe_mach_ticks(), wait_ns));
        } else {
            LOST.fetch_add(1, Ordering::Relaxed);
        }
    }
}
