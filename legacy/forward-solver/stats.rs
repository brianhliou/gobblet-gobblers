//! Solver statistics tracking.

use std::time::Instant;

/// Get current process memory usage in bytes (RSS - Resident Set Size).
/// Returns None if unable to determine.
#[cfg(target_os = "macos")]
pub fn get_memory_usage() -> Option<u64> {
    use std::mem::MaybeUninit;

    // macOS: use mach APIs
    extern "C" {
        fn mach_task_self() -> u32;
        fn task_info(
            target_task: u32,
            flavor: i32,
            task_info_out: *mut libc::c_void,
            task_info_outCnt: *mut u32,
        ) -> i32;
    }

    #[repr(C)]
    struct TaskBasicInfo {
        suspend_count: i32,
        virtual_size: u64,
        resident_size: u64,
        user_time: (i32, i32),
        system_time: (i32, i32),
        policy: i32,
    }

    const TASK_BASIC_INFO_64: i32 = 5;
    const TASK_BASIC_INFO_64_COUNT: u32 = 10;

    unsafe {
        let mut info = MaybeUninit::<TaskBasicInfo>::uninit();
        let mut count = TASK_BASIC_INFO_64_COUNT;

        let result = task_info(
            mach_task_self(),
            TASK_BASIC_INFO_64,
            info.as_mut_ptr() as *mut libc::c_void,
            &mut count,
        );

        if result == 0 {
            Some(info.assume_init().resident_size)
        } else {
            None
        }
    }
}

#[cfg(target_os = "linux")]
pub fn get_memory_usage() -> Option<u64> {
    // Linux: read from /proc/self/status
    use std::fs;

    let status = fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if line.starts_with("VmRSS:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let kb: u64 = parts[1].parse().ok()?;
                return Some(kb * 1024);
            }
        }
    }
    None
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn get_memory_usage() -> Option<u64> {
    None
}

/// Format bytes as human-readable string.
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Statistics collected during solving.
#[derive(Debug, Default)]
pub struct SolverStats {
    /// Positions where we computed the outcome by examining children
    pub positions_evaluated: u64,

    /// Cache hits (position already in transposition table)
    pub cache_hits: u64,

    /// Terminal positions (game ended - win/loss)
    pub terminal_positions: u64,

    /// Cycle draws detected (position already on current path)
    pub cycle_draws: u64,

    /// Maximum stack depth reached
    pub max_depth: u64,

    /// Branches pruned by alpha-beta
    pub branches_pruned: u64,

    /// Breakdown of terminal outcomes
    pub p1_wins: u64,
    pub p2_wins: u64,
    pub draws: u64,

    /// For rate calculation
    start_time: Option<Instant>,
    last_log_time: Option<Instant>,
    last_log_positions: u64,
}

impl SolverStats {
    pub fn new() -> Self {
        Self {
            start_time: Some(Instant::now()),
            last_log_time: Some(Instant::now()),
            ..Default::default()
        }
    }

    /// Record a terminal position outcome
    pub fn record_terminal(&mut self, outcome: i8) {
        self.terminal_positions += 1;
        match outcome {
            1 => self.p1_wins += 1,
            -1 => self.p2_wins += 1,
            0 => self.draws += 1,
            _ => {}
        }
    }

    /// Get current positions per second
    pub fn positions_per_sec(&self) -> f64 {
        if let Some(start) = self.start_time {
            let elapsed = start.elapsed().as_secs_f64();
            if elapsed > 0.0 {
                return self.positions_evaluated as f64 / elapsed;
            }
        }
        0.0
    }

    /// Check if we should log progress
    pub fn should_log(&self, interval_secs: u64) -> bool {
        if let Some(last) = self.last_log_time {
            last.elapsed().as_secs() >= interval_secs
        } else {
            true
        }
    }

    /// Log progress and reset log timer
    pub fn log_progress(&mut self, table_size: usize) {
        let now = Instant::now();
        let elapsed_total = self.start_time.map(|s| s.elapsed().as_secs()).unwrap_or(0);

        // Calculate rate since last log
        let rate = if let Some(last) = self.last_log_time {
            let elapsed = last.elapsed().as_secs_f64();
            let positions = self.positions_evaluated - self.last_log_positions;
            if elapsed > 0.0 {
                positions as f64 / elapsed
            } else {
                0.0
            }
        } else {
            self.positions_per_sec()
        };

        let pruning_pct = if self.positions_evaluated > 0 {
            100.0 * self.branches_pruned as f64
                / (self.positions_evaluated + self.branches_pruned) as f64
        } else {
            0.0
        };

        let mem_str = get_memory_usage()
            .map(|m| format!(" mem={}", format_bytes(m)))
            .unwrap_or_default();

        println!(
            "[{:02}:{:02}:{:02}] positions={} unique={} cache_hits={} rate={:.0}/s depth={} pruned={:.1}%{}",
            elapsed_total / 3600,
            (elapsed_total % 3600) / 60,
            elapsed_total % 60,
            self.positions_evaluated,
            table_size,
            self.cache_hits,
            rate,
            self.max_depth,
            pruning_pct,
            mem_str,
        );
        println!(
            "           terminals: p1={} p2={} draw={} cycles={}",
            self.p1_wins, self.p2_wins, self.draws, self.cycle_draws
        );

        self.last_log_time = Some(now);
        self.last_log_positions = self.positions_evaluated;
    }

    /// Print final summary
    pub fn print_summary(&self) {
        println!("Positions evaluated: {}", self.positions_evaluated);
        println!("Cache hits: {}", self.cache_hits);
        println!("Terminal positions: {}", self.terminal_positions);
        println!("  - P1 wins: {}", self.p1_wins);
        println!("  - P2 wins: {}", self.p2_wins);
        println!("  - Draws: {}", self.draws);
        println!("Cycle draws: {}", self.cycle_draws);
        println!("Max depth: {}", self.max_depth);
        println!("Branches pruned: {}", self.branches_pruned);

        if let Some(start) = self.start_time {
            let elapsed = start.elapsed().as_secs_f64();
            println!(
                "Average rate: {:.0} positions/sec",
                self.positions_evaluated as f64 / elapsed
            );
        }
    }
}
