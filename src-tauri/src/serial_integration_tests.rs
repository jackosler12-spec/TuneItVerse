//! Serial integration tests — UDS application layer through ISO-TP/ELM path.
//!
//! Exercises `uds::*` → `can::uds_request` → `MockSerialPort` end-to-end
//! without hardware. Optional live-port checks when `TUNEITVERSE_SERIAL_PORT` is set.

use super::mock_serial::{boxed_mock, try_open_live_port, MockSerialPort};
use crate::uds::{
    diagnostic_session_control, read_data_by_identifier, read_memory_legacy42, tester_present,
    Alfi, DiagnosticSession, SID_DSC, SID_RDBI, SID_RMBA, SID_TP,
};
use crate::can::{elm_init_can_500k, uds_request, uds_request_multiframe};
use serialport::SerialPort;

fn mock_port() -> Box<dyn SerialPort + Send> {
    boxed_mock(MockSerialPort::new())
}

#[test]
fn elm_init_accepts_at_sequence() {
    let mut port = mock_port();
    assert!(elm_init_can_500k(&mut port).is_ok());
}

#[test]
fn uds_session_control_via_elm() {
    let mut port = mock_port();
    let resp = uds_request(&mut port, SID_DSC, &[0x03], true).expect("session");
    assert!(
        resp == vec![0x50, 0x03] || resp == vec![0x03] || resp.first() == Some(&0x50),
        "unexpected session response: {:02X?}",
        resp
    );
}

#[test]
fn uds_tester_present_via_elm() {
    let mut port = mock_port();
    let resp = uds_request(&mut port, SID_TP, &[0x00], true).expect("tp");
    assert!(
        resp.first() == Some(&0x7E) || resp == vec![0x00],
        "unexpected TP response: {:02X?}",
        resp
    );
}

#[test]
fn uds_rdbi_via_elm() {
    let mut port = mock_port();
    let resp = uds_request_multiframe(&mut port, SID_RDBI, &[0xF1, 0x90], true).expect("rdbi");
    assert!(
        resp.contains(&0x62) || resp.contains(&0xDE),
        "unexpected RDBI: {:02X?}",
        resp
    );
}

#[test]
fn uds_rmba_legacy_via_elm() {
    let mut port = mock_port();
    let mut data = vec![0x42];
    data.extend_from_slice(&0x0008_0000u32.to_be_bytes());
    data.extend_from_slice(&0x0004u16.to_be_bytes());
    let resp = uds_request_multiframe(&mut port, SID_RMBA, &data, true).expect("rmba");
    assert!(
        resp.first() == Some(&0x63) || resp.contains(&0x11),
        "unexpected RMBA: {:02X?}",
        resp
    );
}

#[test]
fn uds_nrc_scripted() {
    let mut mock = MockSerialPort::new().with_auto_elm(false);
    mock.push_elm_hex(&[0x03, 0x7F, 0x10, 0x22]);
    let mut port = boxed_mock(mock);
    let resp = uds_request(&mut port, SID_DSC, &[0x03], true).expect("got frame");
    assert_eq!(resp.first(), Some(&0x7F));
    assert_eq!(resp.get(2), Some(&0x22));
}

#[test]
fn helper_diagnostic_session_extended() {
    let mut port = mock_port();
    let payload = diagnostic_session_control(&mut port, DiagnosticSession::Extended, true)
        .expect("extended session");
    assert!(payload.is_empty() || payload == vec![0x03] || payload.first() == Some(&0x03));
}

#[test]
fn helper_tester_present_suppress() {
    let mut port = mock_port();
    assert!(tester_present(&mut port, true, true).is_ok());
}

#[test]
fn helper_tester_present_with_response() {
    let mut port = mock_port();
    assert!(tester_present(&mut port, false, true).is_ok());
}

#[test]
fn helper_read_did() {
    let mut port = mock_port();
    let data = read_data_by_identifier(&mut port, 0xF190, true).expect("did");
    assert_eq!(data, vec![0xDE, 0xAD]);
}

#[test]
fn helper_read_memory_legacy42() {
    let mut port = mock_port();
    let data = read_memory_legacy42(&mut port, 0x0008_0000, 0x04, true).expect("mem");
    assert_eq!(data, vec![0x11, 0x22, 0x33, 0x44]);
}

#[test]
fn helper_request_download_parses_max_block() {
    let mut port = mock_port();
    let max = crate::uds::request_download(&mut port, Alfi::ADDR4_SIZE4, 0x80000, 0x1000, true)
        .expect("rd");
    assert_eq!(max, 0x0402);
}

#[test]
fn helper_transfer_data_and_exit() {
    let mut port = mock_port();
    crate::uds::transfer_data(&mut port, 1, &[0xAA, 0xBB], true).expect("td");
    crate::uds::transfer_exit(&mut port, true).expect("te");
}

#[test]
fn helper_security_seed_via_raw() {
    let mut port = mock_port();
    let resp = uds_request(&mut port, 0x27, &[0x01], true).expect("seed");
    assert_eq!(resp.first(), Some(&0x67));
    assert!(resp.len() >= 6);
}

#[test]
fn helper_routine_control() {
    let mut port = mock_port();
    let _ = crate::uds::routine_control(&mut port, 0x01, 0xFF00, &[], true).expect("rc");
}

#[test]
fn helper_clear_dtc() {
    let mut port = mock_port();
    assert!(crate::uds::clear_diagnostic_information(&mut port, 0xFFFFFF, true).is_ok());
}

#[test]
fn helper_communication_control() {
    let mut port = mock_port();
    assert!(crate::uds::communication_control(&mut port, 0x03, 0x01, true).is_ok());
}

#[test]
fn helper_control_dtc_setting() {
    let mut port = mock_port();
    assert!(crate::uds::control_dtc_setting(&mut port, 0x02, true).is_ok());
}

#[test]
fn prepare_and_restore_programming_environment() {
    let mut port = mock_port();
    assert!(crate::uds::prepare_programming_environment(&mut port, true, true).is_ok());
    assert!(crate::uds::restore_default_environment(&mut port, true).is_ok());
}

#[test]
fn live_port_session_if_configured() {
    let Some(mut port) = try_open_live_port() else {
        eprintln!("skip live serial test — set TUNEITVERSE_SERIAL_PORT to enable");
        return;
    };
    let _ = elm_init_can_500k(&mut port);
    match diagnostic_session_control(&mut port, DiagnosticSession::Default, true) {
        Ok(p) => eprintln!("live session OK: {:02X?}", p),
        Err(e) => eprintln!("live session no ECU / adapter: {}", e),
    }
}
