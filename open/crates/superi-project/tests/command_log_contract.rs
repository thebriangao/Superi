use superi_core::settings::SemanticVersion;
use superi_project::{
    ProjectCommandLog, ProjectCommandPayloadDisposition, ProjectCommandRecordDraft,
    ProjectCommandRecordKind, MAX_PROJECT_COMMAND_LOG_RECORDS, MAX_RETAINED_PROJECT_COMMAND_BYTES,
};

fn draft(transaction_id: &str, request: &[u8]) -> ProjectCommandRecordDraft {
    ProjectCommandRecordDraft::from_serialized_request(
        transaction_id,
        "superi.project.command.execute",
        SemanticVersion::new(1, 1, 0),
        ProjectCommandRecordKind::Inspect,
        7,
        request,
    )
    .unwrap()
}

#[test]
fn records_are_monotonic_correlated_and_round_trip_exactly() {
    let mut log = ProjectCommandLog::new();
    let before_hash = [0x11; 32];
    let after_hash = before_hash;
    let request = br#"{"transaction_id":"inspect-1"}"#;

    let record = log
        .append(
            draft("inspect-1", request),
            41,
            7,
            7,
            before_hash,
            after_hash,
            false,
        )
        .unwrap();

    assert_eq!(record.sequence(), 1);
    assert_eq!(record.command_sequence(), 41);
    assert_eq!(record.transaction_id(), "inspect-1");
    assert_eq!(record.method(), "superi.project.command.execute");
    assert_eq!(record.command_kind(), ProjectCommandRecordKind::Inspect);
    assert_eq!(record.expected_project_revision(), 7);
    assert_eq!(record.before_project_revision(), 7);
    assert_eq!(record.after_project_revision(), 7);
    assert_eq!(record.before_semantic_hash(), &before_hash);
    assert_eq!(record.after_semantic_hash(), &after_hash);
    assert!(!record.authored_state_changed());
    assert_eq!(record.request_byte_length(), request.len() as u64);
    assert_eq!(
        record.payload_disposition(),
        ProjectCommandPayloadDisposition::Retained
    );
    assert_eq!(record.replay_request(), Some(request.as_slice()));
    assert_eq!(log.oldest_sequence(), Some(1));
    assert_eq!(log.latest_sequence(), 1);

    let encoded = log.encode().unwrap();
    let decoded = ProjectCommandLog::decode(&encoded).unwrap();
    assert_eq!(decoded, log);
}

#[test]
fn oversized_requests_are_digest_only_without_rejecting_the_record() {
    let request = vec![b'x'; MAX_RETAINED_PROJECT_COMMAND_BYTES + 1];
    let mut log = ProjectCommandLog::new();
    let record = log
        .append(
            draft("oversized", &request),
            9,
            7,
            7,
            [1; 32],
            [1; 32],
            false,
        )
        .unwrap();

    assert_eq!(record.request_byte_length(), request.len() as u64);
    assert_eq!(
        record.payload_disposition(),
        ProjectCommandPayloadDisposition::DigestOnly
    );
    assert_eq!(record.replay_request(), None);
    assert_ne!(record.request_sha256(), &[0; 32]);
}

#[test]
fn bounded_retention_evicts_old_records_without_reusing_sequences() {
    let mut log = ProjectCommandLog::new();
    for index in 0..=MAX_PROJECT_COMMAND_LOG_RECORDS {
        let transaction_id = format!("record-{index}");
        log.append(
            draft(&transaction_id, b"{}"),
            index as u64 + 1,
            7,
            7,
            [2; 32],
            [2; 32],
            false,
        )
        .unwrap();
    }

    assert_eq!(log.len(), MAX_PROJECT_COMMAND_LOG_RECORDS);
    assert_eq!(log.oldest_sequence(), Some(2));
    assert_eq!(
        log.latest_sequence(),
        MAX_PROJECT_COMMAND_LOG_RECORDS as u64 + 1
    );
    assert_eq!(log.records().front().unwrap().sequence(), 2);
}
