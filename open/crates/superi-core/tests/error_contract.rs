use std::error::Error as StdError;
use std::io;

use superi_core::error::{
    Error as SuperiError, ErrorCategory, ErrorContext, Recoverability, Result, ResultExt,
};

#[test]
fn category_codes_are_stable_and_round_trip() {
    let expected = [
        "invalid_input",
        "not_found",
        "conflict",
        "unsupported",
        "permission_denied",
        "resource_exhausted",
        "unavailable",
        "timeout",
        "cancelled",
        "corrupt_data",
        "internal",
    ];

    let actual: Vec<_> = ErrorCategory::ALL
        .iter()
        .map(|category| category.code())
        .collect();
    assert_eq!(actual, expected);

    for category in ErrorCategory::ALL {
        assert_eq!(ErrorCategory::from_code(category.code()), Some(*category));
        assert_eq!(category.to_string(), category.code());
    }
    assert_eq!(ErrorCategory::from_code("unknown"), None);
}

#[test]
fn recoverability_codes_are_stable_and_complete() {
    let expected = ["retryable", "degraded", "user_correctable", "terminal"];
    let actual: Vec<_> = Recoverability::ALL
        .iter()
        .map(|value| value.code())
        .collect();
    assert_eq!(actual, expected);

    for value in Recoverability::ALL {
        assert_eq!(Recoverability::from_code(value.code()), Some(*value));
        assert_eq!(value.to_string(), value.code());
    }
    assert_eq!(Recoverability::from_code("unknown"), None);
}

#[test]
fn context_is_owned_and_deterministic() {
    let mut context = ErrorContext::new("superi-project", "save")
        .with_field("revision", "42")
        .with_field("path", "/tmp/project.superi");

    assert_eq!(context.component(), "superi-project");
    assert_eq!(context.operation(), "save");
    assert_eq!(context.field("path"), Some("/tmp/project.superi"));
    assert_eq!(
        context.insert_field("revision", "43"),
        Some("42".to_owned())
    );
    assert_eq!(
        context.to_string(),
        "superi-project.save(path=/tmp/project.superi, revision=43)"
    );
}

#[test]
fn shared_error_retains_classification_and_context() {
    let error = SuperiError::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        "project changed while saving",
    )
    .with_context(
        ErrorContext::new("superi-project", "save").with_field("path", "/tmp/project.superi"),
    )
    .with_context(ErrorContext::new("superi-api", "dispatch").with_field("request_id", "7"));

    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(error.message(), "project changed while saving");
    assert_eq!(error.contexts()[0].component(), "superi-project");
    assert_eq!(error.contexts()[1].component(), "superi-api");
    assert_eq!(
        error.to_string(),
        "project changed while saving [superi-api.dispatch(request_id=7) -> \
         superi-project.save(path=/tmp/project.superi)]"
    );
}

#[test]
fn standard_source_chain_retains_every_boundary() {
    let leaf = io::Error::new(io::ErrorKind::UnexpectedEof, "truncated project database");
    let inner = SuperiError::with_source(
        ErrorCategory::CorruptData,
        Recoverability::UserCorrectable,
        "project database could not be read",
        leaf,
    );
    let outer = SuperiError::with_source(
        ErrorCategory::Internal,
        Recoverability::Terminal,
        "project open failed",
        inner,
    );

    let inner_source = StdError::source(&outer).expect("inner source");
    let inner_error = inner_source
        .downcast_ref::<SuperiError>()
        .expect("shared inner error");
    assert_eq!(inner_error.category(), ErrorCategory::CorruptData);

    let leaf_source = StdError::source(inner_error).expect("leaf source");
    assert_eq!(leaf_source.to_string(), "truncated project database");
    assert!(!outer.to_string().contains("truncated project database"));
}

#[test]
fn result_context_preserves_the_original_failure() {
    fn fail() -> Result<()> {
        Err(SuperiError::with_source(
            ErrorCategory::Unavailable,
            Recoverability::Retryable,
            "cache file is temporarily unavailable",
            io::Error::new(io::ErrorKind::WouldBlock, "file is locked"),
        ))
    }

    let error = fail()
        .with_error_context(
            ErrorContext::new("superi-cache", "load_frame").with_field("frame", "120"),
        )
        .expect_err("failure must propagate");

    assert_eq!(error.category(), ErrorCategory::Unavailable);
    assert_eq!(error.recoverability(), Recoverability::Retryable);
    assert_eq!(error.message(), "cache file is temporarily unavailable");
    assert_eq!(error.contexts().len(), 1);
    assert_eq!(
        StdError::source(&error).unwrap().to_string(),
        "file is locked"
    );
}

#[test]
fn shared_error_values_are_thread_safe() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<SuperiError>();
    assert_send_sync::<ErrorContext>();
}
