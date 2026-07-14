use std::fs;
use std::process::ExitCode;

use superi_bench::{
    register_graph_evaluation_workload, BenchmarkConfig, BenchmarkContext, BenchmarkContextFields,
    BenchmarkStage, BenchmarkSuite,
};

fn main() -> ExitCode {
    match run() {
        Ok(has_failures) if has_failures => ExitCode::FAILURE,
        Ok(_) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("superi-bench: {message}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<bool, String> {
    let warmup = number("SUPERI_BENCH_WARMUP", 5)?;
    let samples = number("SUPERI_BENCH_SAMPLES", 20)?;
    let mut config = BenchmarkConfig::new(warmup, samples).map_err(|error| error.to_string())?;
    if let Ok(filter) = std::env::var("SUPERI_BENCH_STAGES") {
        let stages = filter
            .split(',')
            .map(str::trim)
            .map(|code| {
                BenchmarkStage::from_code(code).ok_or_else(|| format!("unknown stage {code:?}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        config = config
            .with_stages(stages)
            .map_err(|error| error.to_string())?;
    }

    let context = BenchmarkContext::new(BenchmarkContextFields {
        build: text(
            "SUPERI_BENCH_BUILD",
            concat!(env!("CARGO_PKG_VERSION"), "-bench"),
        ),
        operating_system: text("SUPERI_BENCH_OS", std::env::consts::OS),
        architecture: text("SUPERI_BENCH_ARCH", std::env::consts::ARCH),
        cpu: text("SUPERI_BENCH_CPU", "unreported"),
        memory_mib: number("SUPERI_BENCH_MEMORY_MIB", 1)?.into(),
        gpu_backend: text("SUPERI_BENCH_GPU_BACKEND", "unreported"),
        gpu_driver: text("SUPERI_BENCH_GPU_DRIVER", "unreported"),
        cache_state: text("SUPERI_BENCH_CACHE_STATE", "unreported"),
        hardware_tier: text("SUPERI_BENCH_HARDWARE_TIER", "unreported"),
        fixture_revision: text("SUPERI_BENCH_FIXTURE_REVISION", "unreported"),
        project_revision: text("SUPERI_BENCH_PROJECT_REVISION", "unreported"),
    })
    .map_err(|error| error.to_string())?;

    let mut suite = BenchmarkSuite::new();
    register_graph_evaluation_workload(&mut suite).map_err(|error| error.to_string())?;
    let report = suite.run(&config, &context);
    let json = report.to_json();
    if let Ok(path) = std::env::var("SUPERI_BENCH_REPORT") {
        fs::write(&path, format!("{json}\n"))
            .map_err(|error| format!("cannot write report {path:?}: {error}"))?;
    }
    println!("{json}");
    Ok(report.has_failures())
}

fn text(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.into())
}

fn number(name: &str, default: u32) -> Result<u32, String> {
    std::env::var(name).map_or(Ok(default), |value| {
        value
            .parse::<u32>()
            .map_err(|error| format!("invalid {name}: {error}"))
    })
}
