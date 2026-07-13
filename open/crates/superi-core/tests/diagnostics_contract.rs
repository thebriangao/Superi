use std::io;
use std::sync::Arc;
use std::thread;

use superi_core::diagnostics::{
    CounterUnit, DiagnosticEvent, DiagnosticSeverity, FieldVisibility, FiniteF64,
    PerformanceCounter, TraceField, TraceValue, UserSafeError,
};
use superi_core::error::{Error as SuperiError, ErrorCategory, ErrorContext, Recoverability};

#[test]
fn stable_diagnostic_codes_round_trip() {
    let severities = ["trace", "debug", "info", "warning", "error"];
    assert_eq!(
        DiagnosticSeverity::ALL
            .iter()
            .map(|value| value.code())
            .collect::<Vec<_>>(),
        severities
    );
    for value in DiagnosticSeverity::ALL {
        assert_eq!(DiagnosticSeverity::from_code(value.code()), Some(*value));
    }
    assert_eq!(DiagnosticSeverity::from_code("fatal"), None);

    let visibilities = ["user_safe", "internal", "sensitive"];
    assert_eq!(
        FieldVisibility::ALL
            .iter()
            .map(|value| value.code())
            .collect::<Vec<_>>(),
        visibilities
    );
    for value in FieldVisibility::ALL {
        assert_eq!(FieldVisibility::from_code(value.code()), Some(*value));
    }

    let units = ["count", "bytes", "nanoseconds", "frames", "samples"];
    assert_eq!(
        CounterUnit::ALL
            .iter()
            .map(|value| value.code())
            .collect::<Vec<_>>(),
        units
    );
    for value in CounterUnit::ALL {
        assert_eq!(CounterUnit::from_code(value.code()), Some(*value));
    }
}

#[test]
fn events_own_typed_fields_in_canonical_order() {
    let event = DiagnosticEvent::new(
        "render.frame.completed",
        "superi-engine.render",
        DiagnosticSeverity::Info,
        "frame completed",
    )
    .unwrap()
    .with_field("z.private", TraceField::sensitive("/private/media.mov"))
    .unwrap()
    .with_field("a.frame", TraceField::user_safe(42_u64))
    .unwrap()
    .with_field("m.cached", TraceField::internal(true))
    .unwrap()
    .with_field(
        "m.ratio",
        TraceField::internal(TraceValue::try_from(-0.0_f64).unwrap()),
    )
    .unwrap();

    assert_eq!(event.name(), "render.frame.completed");
    assert_eq!(event.component(), "superi-engine.render");
    assert_eq!(event.severity(), DiagnosticSeverity::Info);
    assert_eq!(event.message(), "frame completed");
    assert_eq!(
        event
            .fields()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["a.frame", "m.cached", "m.ratio", "z.private"]
    );
    assert_eq!(
        event.field("a.frame").unwrap().value(),
        &TraceValue::Unsigned(42)
    );
    assert_eq!(
        event
            .user_safe_fields()
            .map(|(name, _)| name)
            .collect::<Vec<_>>(),
        ["a.frame"]
    );

    let TraceValue::Float(value) = event.field("m.ratio").unwrap().value() else {
        panic!("expected finite float");
    };
    assert_eq!(value.get().to_bits(), 0.0_f64.to_bits());
}

#[test]
fn names_and_floating_point_values_are_canonical() {
    for valid in ["render", "render.frame", "cache-hit", "worker_2"] {
        DiagnosticEvent::new(valid, "superi-core", DiagnosticSeverity::Debug, "ok").unwrap();
        PerformanceCounter::new(valid, CounterUnit::Count).unwrap();
    }

    for invalid in [
        "",
        "Render",
        ".render",
        "render.",
        "render..frame",
        "render frame",
    ] {
        let error =
            DiagnosticEvent::new(invalid, "superi-core", DiagnosticSeverity::Debug, "invalid")
                .unwrap_err();
        assert_eq!(error.category(), ErrorCategory::InvalidInput);
        assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    }

    assert!(FiniteF64::new(f64::NAN).is_err());
    assert!(FiniteF64::new(f64::INFINITY).is_err());
    assert_eq!(FiniteF64::new(1.25).unwrap().get(), 1.25);
}

#[test]
fn error_events_retain_internal_evidence_without_leaking_it_to_users() {
    let secret_path = "/Users/editor/unreleased-film.superi";
    let leaf = io::Error::new(io::ErrorKind::PermissionDenied, "token=super-secret");
    let inner = SuperiError::with_source(
        ErrorCategory::PermissionDenied,
        Recoverability::UserCorrectable,
        "project database access failed",
        leaf,
    );
    let outer = SuperiError::with_source(
        ErrorCategory::Internal,
        Recoverability::Terminal,
        "project open failed for unreleased film",
        inner,
    )
    .with_context(ErrorContext::new("superi-project", "open").with_field("path", secret_path));

    let event = DiagnosticEvent::from_error("project.open.failed", "superi-project", &outer)
        .expect("valid event");
    let failure = event.failure().expect("failure snapshot");
    assert_eq!(failure.category(), ErrorCategory::Internal);
    assert_eq!(failure.recoverability(), Recoverability::Terminal);
    assert_eq!(failure.summary(), "project open failed for unreleased film");
    assert_eq!(failure.contexts()[0].field("path"), Some(secret_path));
    assert_eq!(failure.source_summaries().len(), 2);
    assert!(failure.source_summaries()[1].contains("token=super-secret"));

    let safe = event.user_safe_error().expect("safe error");
    let rendered = safe.to_string();
    assert_eq!(safe.category(), ErrorCategory::Internal);
    assert_eq!(safe.recoverability(), Recoverability::Terminal);
    assert_eq!(safe.code(), "error.internal.terminal");
    assert!(!rendered.contains(secret_path));
    assert!(!rendered.contains("unreleased"));
    assert!(!rendered.contains("token"));
    assert_eq!(event.severity(), DiagnosticSeverity::Error);
}

#[test]
fn safe_error_projection_preserves_every_recovery_classification() {
    let cases = [
        (Recoverability::Retryable, DiagnosticSeverity::Warning),
        (Recoverability::Degraded, DiagnosticSeverity::Warning),
        (Recoverability::UserCorrectable, DiagnosticSeverity::Warning),
        (Recoverability::Terminal, DiagnosticSeverity::Error),
    ];

    for (recoverability, severity) in cases {
        let error = SuperiError::new(ErrorCategory::Unavailable, recoverability, "raw detail");
        let safe = UserSafeError::from_error(&error);
        assert_eq!(safe.category(), ErrorCategory::Unavailable);
        assert_eq!(safe.recoverability(), recoverability);
        assert!(safe.code().ends_with(recoverability.code()));

        let event = DiagnosticEvent::from_error("engine.operation.failed", "superi-engine", &error)
            .unwrap();
        assert_eq!(event.severity(), severity);
        assert_eq!(event.failure().unwrap().recoverability(), recoverability);
    }
}

#[test]
fn performance_counters_are_monotonic_saturating_and_thread_safe() {
    let saturated =
        PerformanceCounter::with_initial_value("render.frames", CounterUnit::Frames, u64::MAX - 1)
            .unwrap();
    assert_eq!(saturated.add(10), u64::MAX);
    assert_eq!(saturated.increment(), u64::MAX);
    assert_eq!(saturated.value(), u64::MAX);

    let counter = Arc::new(PerformanceCounter::new("cache.hits", CounterUnit::Count).unwrap());
    let workers = (0..8)
        .map(|_| {
            let counter = Arc::clone(&counter);
            thread::spawn(move || {
                for _ in 0..1_000 {
                    counter.increment();
                }
            })
        })
        .collect::<Vec<_>>();
    for worker in workers {
        worker.join().unwrap();
    }

    let snapshot = counter.snapshot();
    assert_eq!(snapshot.name(), "cache.hits");
    assert_eq!(snapshot.unit(), CounterUnit::Count);
    assert_eq!(snapshot.value(), 8_000);
    assert_eq!(counter.value(), 8_000);
}

#[test]
fn diagnostic_contracts_are_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<DiagnosticEvent>();
    assert_send_sync::<TraceField>();
    assert_send_sync::<UserSafeError>();
    assert_send_sync::<PerformanceCounter>();
}
