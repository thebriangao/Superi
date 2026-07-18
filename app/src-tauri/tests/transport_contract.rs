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
