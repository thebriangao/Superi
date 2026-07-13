use std::io::{self, BufRead, Write};
use std::thread;
use std::time::Duration;

fn main() {
    let hang = std::env::args().any(|argument| argument == "--hang");
    let invalid_json = std::env::args().any(|argument| argument == "--invalid-json");
    let unterminated = std::env::args().any(|argument| argument == "--unterminated");
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    let mut packet_sent = false;
    let mut frame_sent = false;
    let mut flushed = false;

    for line in stdin.lock().lines() {
        let line = line.expect("read request");
        let id = request_id(&line);
        if line.contains("\"kind\":\"handshake\"") {
            if hang {
                thread::sleep(Duration::from_secs(10));
            }
            if invalid_json {
                writeln!(stdout, "not-json").expect("write invalid response");
                stdout.flush().expect("flush invalid response");
                return;
            }
            if unterminated {
                write!(stdout, "{{\"id\":{id},\"payload\":{{\"kind\":\"ack\"}}}}")
                    .expect("write unterminated response");
                stdout.flush().expect("flush unterminated response");
                return;
            }
            respond(
                &mut stdout,
                id,
                r#"{"kind":"handshake","manifest":{"protocol_revision":1,"backend_id":"test-vendor-raw","display_name":"Test Vendor RAW","plugin_version":"1.0.0","sdk_version":"fixture-1","formats":["arriraw","r3d","braw"]}}"#,
            );
        } else if line.contains("\"kind\":\"probe\"") {
            respond(
                &mut stdout,
                id,
                r#"{"kind":"probe","result":{"kind":"match","format":"braw","confidence":100}}"#,
            );
        } else if line.contains("\"kind\":\"open\"") {
            respond(
                &mut stdout,
                id,
                r#"{"kind":"open","source":{"source_handle":"source-1","fingerprint":"sha256:fixture","streams":[{"id":0,"kind":"video","codec":"braw","timebase":{"numerator":24,"denominator":1},"duration":2,"metadata":{"camera.model":{"kind":"text","value":"ursa-mini-pro-12k"}}}],"duration":{"value":2,"timebase":{"numerator":24,"denominator":1}},"metadata":{"vendor.sidecar":{"kind":"boolean","value":true}}}}"#,
            );
        } else if line.contains("\"kind\":\"read_packet\"") {
            if packet_sent {
                respond(
                    &mut stdout,
                    id,
                    r#"{"kind":"read_packet","outcome":{"kind":"end_of_stream"}}"#,
                );
            } else {
                packet_sent = true;
                respond(
                    &mut stdout,
                    id,
                    r#"{"kind":"read_packet","outcome":{"kind":"complete","packet":{"stream_id":0,"data_hex":"0102","timing":{"timebase":{"numerator":24,"denominator":1},"presentation":0,"decode":0,"duration":1},"keyframe":true,"metadata":{"vendor.frame_number":{"kind":"unsigned","value":0}}}}}"#,
                );
            }
        } else if line.contains("\"kind\":\"seek\"") {
            packet_sent = false;
            respond(
                &mut stdout,
                id,
                r#"{"kind":"seek","selected":{"value":1,"timebase":{"numerator":24,"denominator":1}}}"#,
            );
        } else if line.contains("\"kind\":\"create_decoder\"") {
            frame_sent = false;
            flushed = false;
            respond(
                &mut stdout,
                id,
                r#"{"kind":"decoder_created","decoder_handle":"decoder-1"}"#,
            );
        } else if line.contains("\"kind\":\"send_packet\"") {
            frame_sent = false;
            respond(&mut stdout, id, r#"{"kind":"ack"}"#);
        } else if line.contains("\"kind\":\"receive_decoder\"") {
            if flushed {
                respond(
                    &mut stdout,
                    id,
                    r#"{"kind":"decoder_output","output":{"kind":"end_of_stream"}}"#,
                );
            } else if frame_sent {
                respond(
                    &mut stdout,
                    id,
                    r#"{"kind":"decoder_output","output":{"kind":"need_input"}}"#,
                );
            } else {
                frame_sent = true;
                respond(
                    &mut stdout,
                    id,
                    r#"{"kind":"decoder_output","output":{"kind":"frame","frame":{"width":2,"height":1,"pixel_format":"rgba16_float","color_space":{"primaries":"aces_ap1","transfer":"linear","matrix":"rgb","range":"full"},"alpha_mode":"straight","timestamp":{"value":0,"timebase":{"numerator":24,"denominator":1}},"duration":{"value":1,"timebase":{"numerator":24,"denominator":1}},"planes":[{"data_hex":"000000000000003c000000000000003c","stride":16,"row_count":1}],"metadata":{"vendor.iso":{"kind":"unsigned","value":800}}}}}"#,
                );
            }
        } else if line.contains("\"kind\":\"flush_decoder\"") {
            flushed = true;
            respond(&mut stdout, id, r#"{"kind":"ack"}"#);
        } else if line.contains("\"kind\":\"reset_decoder\"") {
            frame_sent = false;
            flushed = false;
            respond(&mut stdout, id, r#"{"kind":"ack"}"#);
        } else if line.contains("\"kind\":\"close_source\"")
            || line.contains("\"kind\":\"close_decoder\"")
        {
            respond(&mut stdout, id, r#"{"kind":"ack"}"#);
        } else {
            respond(
                &mut stdout,
                id,
                r#"{"kind":"failure","error":{"category":"unsupported","recoverability":"degraded","message":"fixture received an unsupported request"}}"#,
            );
        }
    }
}

fn request_id(line: &str) -> u64 {
    let prefix = "{\"id\":";
    let rest = line.strip_prefix(prefix).expect("request id prefix");
    rest.split(',')
        .next()
        .expect("request id")
        .parse()
        .expect("numeric request id")
}

fn respond(stdout: &mut impl Write, id: u64, payload: &str) {
    writeln!(stdout, "{{\"id\":{id},\"payload\":{payload}}}").expect("write response");
    stdout.flush().expect("flush response");
}
