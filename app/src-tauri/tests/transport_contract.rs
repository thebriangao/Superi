use superi_desktop::engine::LinkedEngineProcess;
use superi_desktop::lifecycle::ApplicationLifecycle;
use superi_desktop::project_lifecycle::{
    DesktopProjectCommand, DesktopProjectCreateRequest, DesktopProjectState,
};
use superi_desktop::transport::{
    DesktopTransportCommand, DesktopTransportReply, DesktopTransportState,
};

#[test]
fn transport_state_opens_one_ordered_connection_generation() {
    let transport = DesktopTransportState::new();
    let reply = transport
        .dispatch_control(DesktopTransportCommand::Connect { after_sequence: 0 })
        .unwrap();

    let DesktopTransportReply::Connected {
        generation,
        stream_id,
        replay,
        resync_required,
    } = reply
    else {
        panic!("connect returned an unexpected transport reply");
    };
    assert_eq!(generation, 1);
    assert_eq!(stream_id, "superi.desktop.events.v1");
    assert!(replay.is_empty());
    assert!(!resync_required);
}

#[test]
fn active_project_state_track_and_marker_commands_use_the_generated_desktop_route() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "superi-desktop-track-route-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let project_path = root.join("project.superi");

    let projects = DesktopProjectState::default();
    projects.initialize(root.join("recovery")).unwrap();
    let project_id = superi_engine::editor::ProjectId::from_raw(1).to_string();
    let timeline_id = superi_engine::editor::TimelineId::from_raw(2).to_string();
    let track_id = superi_engine::editor::TrackId::from_raw(3).to_string();
    let marker_id = superi_engine::editor::MarkerId::from_raw(4).to_string();
    projects
        .execute(DesktopProjectCommand::Create {
            path: project_path.to_string_lossy().into_owned(),
            project: DesktopProjectCreateRequest {
                project_id,
                project_name: "transport project".to_owned(),
                root_timeline_id: timeline_id.clone(),
                root_timeline_name: "main edit".to_owned(),
                edit_rate_numerator: 24,
                edit_rate_denominator: 1,
            },
        })
        .unwrap();

    let lifecycle = ApplicationLifecycle::new().unwrap();
    let engine = LinkedEngineProcess::launch(lifecycle.clone()).unwrap();
    let transport = DesktopTransportState::new();
    let DesktopTransportReply::Connected { generation, .. } = transport
        .dispatch_control(DesktopTransportCommand::Connect { after_sequence: 0 })
        .unwrap()
    else {
        panic!("connect returned an unexpected reply");
    };

    let state = transport
        .dispatch_blocking(
            &engine.connection(),
            &projects,
            DesktopTransportCommand::Request {
                generation,
                request_id: "editor-state-1".to_owned(),
                method: "superi.editor.state.get".to_owned(),
                request: serde_json::json!({"transaction_id":"editor-state-1"}),
            },
        )
        .unwrap();
    assert!(state.event().is_none());
    let DesktopTransportReply::Response { response, .. } = state.reply() else {
        panic!("editor state returned an unexpected reply");
    };
    assert_eq!(response["snapshot"]["project"]["project_revision"], 0);

    let command = transport
        .dispatch_blocking(
            &engine.connection(),
            &projects,
            DesktopTransportCommand::Request {
                generation,
                request_id: "track-command-1".to_owned(),
                method: "superi.project.command.execute".to_owned(),
                request: serde_json::json!({
                    "transaction_id":"track-command-1",
                    "expected_project_revision":0,
                    "command":{
                        "command":"apply",
                        "actions":[{
                            "action":"mutate_tracks",
                            "mutations":[{
                                "operation":"create",
                                "timeline_id":timeline_id,
                                "track_id":track_id,
                                "name":"V1",
                                "kind":"video",
                                "position":0,
                                "height":96
                            }]
                        }]
                    }
                }),
            },
        )
        .unwrap();
    let DesktopTransportReply::Response { response, .. } = command.reply() else {
        panic!("track command returned an unexpected reply");
    };
    assert_eq!(response["state"]["project_revision"], 1);
    let event = command
        .event()
        .expect("authored command must publish an event");
    assert_eq!(event.event(), "superi.project.state.changed");
    assert_eq!(event.payload()["project_revision"], 1);

    let marker_command = transport
        .dispatch_blocking(
            &engine.connection(),
            &projects,
            DesktopTransportCommand::Request {
                generation,
                request_id: "marker-command-1".to_owned(),
                method: "superi.project.command.execute".to_owned(),
                request: serde_json::json!({
                    "transaction_id":"marker-command-1",
                    "expected_project_revision":1,
                    "command":{
                        "command":"apply",
                        "actions":[{
                            "action":"mutate_markers",
                            "mutations":[{
                                "operation":"create",
                                "timeline_id":timeline_id,
                                "marker_id":marker_id,
                                "owner":{"kind":"timeline"},
                                "marked_range":{
                                    "start":{
                                        "value":12,
                                        "timebase":{"numerator":24,"denominator":1}
                                    },
                                    "duration":{
                                        "value":1,
                                        "timebase":{"numerator":24,"denominator":1}
                                    }
                                },
                                "label":"First review",
                                "flag":"cyan",
                                "note":"Check the exact cut",
                                "metadata":{}
                            }]
                        }]
                    }
                }),
            },
        )
        .unwrap();
    let DesktopTransportReply::Response { response, .. } = marker_command.reply() else {
        panic!("marker command returned an unexpected reply");
    };
    assert_eq!(response["state"]["project_revision"], 2);
    assert_eq!(
        response["evidence"],
        serde_json::json!({
            "result":"applied",
            "actions":[{
                "result":"markers_mutated",
                "revision":2,
                "mutations":["create"]
            }]
        })
    );
    let marker_event = marker_command
        .event()
        .expect("authored marker command must publish an event");
    assert_eq!(marker_event.event(), "superi.project.state.changed");
    assert_eq!(marker_event.payload()["project_revision"], 2);

    let refreshed = projects
        .inspect_editor(superi_api::commands::GetEditorState::new("editor-state-2"))
        .unwrap();
    let refreshed = serde_json::to_value(refreshed).unwrap();
    assert_eq!(refreshed["snapshot"]["project"]["project_revision"], 2);
    assert_eq!(
        refreshed["snapshot"]["timeline"]["document"]["content"]["payload"]["timelines"][0]
            ["edit_state"]["track_states"][0]["height"],
        96
    );
    assert_eq!(
        refreshed["snapshot"]["timeline"]["document"]["content"]["payload"]["timelines"][0]
            ["markers"][0]["id"],
        marker_id
    );
    assert_eq!(
        refreshed["snapshot"]["timeline"]["document"]["content"]["payload"]["timelines"][0]
            ["markers"][0]["marked_range"]["start"]["value"],
        "12"
    );
    assert_eq!(
        refreshed["snapshot"]["timeline"]["document"]["content"]["payload"]["timelines"][0]
            ["markers"][0]["label"],
        "First review"
    );

    lifecycle.request_shutdown().unwrap();
    engine.join().unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}
