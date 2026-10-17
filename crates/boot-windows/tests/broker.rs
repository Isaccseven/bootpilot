use boot_core::*;
use boot_windows::broker::*;
#[test]
fn unknown_message_is_rejected() {
    assert!(decode_request(br#"{"schema_version":1,"nonce":"x","request":{"action":"shell","command":"anything"}}"#, "x").is_err());
}
#[test]
fn wrong_pipe_peer_is_rejected() {
    assert_eq!(
        verify_peer(
            &PeerIdentity {
                pid: 10,
                session: 1,
                sid: "a".into(),
                image_directory: "C:/app".into()
            },
            &PeerIdentity {
                pid: 11,
                session: 1,
                sid: "a".into(),
                image_directory: "C:/app".into()
            }
        )
        .unwrap_err()
        .code,
        ErrorCode::UntrustedPeer
    );
}
struct Cancelled;
impl BrokerTransport for Cancelled {
    fn exchange(&self, _: BrokerRequest) -> BootResult<BrokerResponse> {
        Err(BootError::new(ErrorCode::UacCancelled, "Abgebrochen"))
    }
}
#[test]
fn uac_cancel_has_no_writes() {
    let client = BrokerClient::with_transport(Cancelled);
    assert_eq!(
        client.request(BrokerRequest::Refresh).unwrap_err().code,
        ErrorCode::UacCancelled
    );
}

#[test]
fn oversize_and_extra_fields_and_nonce_are_rejected() {
    assert_eq!(
        decode_request(&vec![0; MAX_FRAME_BYTES + 1], "x")
            .unwrap_err()
            .code,
        ErrorCode::InvalidMessage
    );
    for message in [
        br#"{"schema_version":1,"nonce":"x","request":{"action":"refresh"},"shell":"x"}"#
            .as_slice(),
        br#"{"schema_version":1,"nonce":"wrong","request":{"action":"refresh"}}"#,
    ] {
        assert!(decode_request(message, "x").is_err());
    }
}
#[test]
fn wrong_account_session_and_directory_are_rejected() {
    let expected = PeerIdentity {
        pid: 1,
        session: 2,
        sid: "S-1-5-21-123".into(),
        image_directory: "C:/app".into(),
    };
    for actual in [
        PeerIdentity {
            session: 3,
            ..expected.clone()
        },
        PeerIdentity {
            sid: "S-1-5-21-456".into(),
            ..expected.clone()
        },
        PeerIdentity {
            image_directory: "C:/other".into(),
            ..expected.clone()
        },
    ] {
        assert!(verify_peer(&expected, &actual).is_err());
    }
}
#[test]
fn prepare_and_apply_cannot_write() {
    for request in [
        BrokerRequest::Apply {
            plan_id: uuid::Uuid::new_v4(),
        },
        BrokerRequest::Prepare(PlanRequest::Menu {
            default_entry_id: "windows".into(),
            timeout_seconds: 5,
        }),
    ] {
        assert_eq!(
            dispatch(request, "S-1-5-21-test").unwrap_err().code,
            ErrorCode::NotImplemented
        );
    }
}
