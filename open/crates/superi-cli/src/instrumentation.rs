//! Bounded stage-boundary instrumentation for the canonical slice runner.

use std::time::Instant;

use serde::Serialize;
use sysinfo::{get_current_pid, Pid, ProcessRefreshKind, ProcessesToUpdate, System};

#[derive(Serialize)]
pub(crate) struct StageMemory {
    resident_bytes_before: u64,
    resident_bytes_after: u64,
}

impl StageMemory {
    pub(crate) fn observed_resident_bytes_max(&self) -> u64 {
        self.resident_bytes_before.max(self.resident_bytes_after)
    }
}

#[derive(Serialize)]
pub(crate) struct StageInstrumentation {
    pub(crate) duration_us: u64,
    pub(crate) memory: StageMemory,
}

#[derive(Serialize)]
pub(crate) struct InstrumentationSummary {
    clock: &'static str,
    duration_unit: &'static str,
    memory_metric: &'static str,
    memory_unit: &'static str,
    sampling: &'static str,
    stage_count: usize,
    observed_resident_bytes_max: u64,
}

impl InstrumentationSummary {
    pub(crate) fn new(stage_count: usize, observed_resident_bytes_max: u64) -> Self {
        Self {
            clock: "monotonic",
            duration_unit: "microseconds",
            memory_metric: "process_resident_set",
            memory_unit: "bytes",
            sampling: "stage_boundaries",
            stage_count,
            observed_resident_bytes_max,
        }
    }
}

pub(crate) struct ProcessMemorySampler {
    system: System,
    pid: Pid,
}

impl ProcessMemorySampler {
    pub(crate) fn new() -> Result<Self, String> {
        let pid = get_current_pid()
            .map_err(|message| format!("could not resolve the current process: {message}"))?;
        Ok(Self {
            system: System::new(),
            pid,
        })
    }

    pub(crate) fn begin_stage(&mut self) -> Result<StageProbe, String> {
        let resident_bytes_before = self.sample_resident_bytes()?;
        Ok(StageProbe {
            started: Instant::now(),
            resident_bytes_before,
        })
    }

    fn sample_resident_bytes(&mut self) -> Result<u64, String> {
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[self.pid]),
            true,
            ProcessRefreshKind::nothing().with_memory().without_tasks(),
        );
        self.system
            .process(self.pid)
            .map(|process| process.memory())
            .filter(|resident_bytes| *resident_bytes > 0)
            .ok_or_else(|| "current process resident memory was unavailable".to_owned())
    }
}

pub(crate) struct StageProbe {
    started: Instant,
    resident_bytes_before: u64,
}

impl StageProbe {
    pub(crate) fn finish(
        self,
        sampler: &mut ProcessMemorySampler,
    ) -> Result<StageInstrumentation, String> {
        let duration_us = u64::try_from(self.started.elapsed().as_micros()).unwrap_or(u64::MAX);
        let resident_bytes_after = sampler.sample_resident_bytes()?;
        Ok(StageInstrumentation {
            duration_us,
            memory: StageMemory {
                resident_bytes_before: self.resident_bytes_before,
                resident_bytes_after,
            },
        })
    }
}
