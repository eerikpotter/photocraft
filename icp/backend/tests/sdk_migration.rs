//! Exercise the extracted SDK against the deployed V1 contract and pre-extraction Wasm.
use candid::{Principal, encode_args};
use futures::executor::block_on;
use pocket_ic::{PocketIc, PocketIcBuilder};
use sovereign_cloud::{CloudResult, FileClient, Transport, UploadState, protocol::CHUNK_BYTES};
use std::cell::Cell;

struct PocketTransport<'a> {
    pic: &'a PocketIc,
    canister: Principal,
    caller: Principal,
    lose_commit_reply: Cell<bool>,
}
impl Transport for PocketTransport<'_> {
    async fn update(&self, method: &str, args: Vec<u8>) -> CloudResult<Vec<u8>> {
        let bytes = self.pic.update_call(self.canister, self.caller, method, args).map_err(|e| format!("{e:?}"))?;
        if method == "commit_upload" && self.lose_commit_reply.replace(false) {
            return Err("Simulated dropped response after consensus".into());
        }
        Ok(bytes)
    }
    async fn query(&self, method: &str, args: Vec<u8>) -> CloudResult<Vec<u8>> {
        self.pic.query_call(self.canister, self.caller, method, args).map_err(|e| format!("{e:?}"))
    }
}

#[test]
fn legacy_save_survives_sdk_upgrade_and_lost_commit_response() {
    let old_wasm = std::fs::read(std::env::var("PHOTOCRAFT_BASELINE_WASM").expect("Set PHOTOCRAFT_BASELINE_WASM to the pre-extraction canister")).unwrap();
    let new_wasm = std::fs::read(std::env::var("PHOTOCRAFT_CLOUD_WASM").expect("Set PHOTOCRAFT_CLOUD_WASM to the updated canister")).unwrap();
    let pic = PocketIcBuilder::new().with_application_subnet().build();
    let canister = pic.create_canister();
    pic.add_cycles(canister, 20_000_000_000_000);
    pic.install_canister(canister, old_wasm, encode_args(()).unwrap(), None);
    let owner = Principal::self_authenticating([31; 32]);
    let client = FileClient::with_transport(PocketTransport { pic: &pic, canister, caller: owner, lose_commit_reply: Cell::new(true) });
    let bytes: Vec<_> = (0..CHUNK_BYTES + 17).map(|i| (i % 251) as u8).collect();
    let mut attempt = UploadState { file: None, upload: None, expected_revision: None, name: "Preserved file".into(), request_id: "sdk-migration-0001".into() };
    assert!(block_on(client.save(&mut attempt, &bytes, |_| {})).is_err());
    assert!(attempt.file.is_some());
    assert!(attempt.upload.is_some());
    pic.upgrade_canister(canister, new_wasm, encode_args(()).unwrap(), None).unwrap();
    let revision = block_on(client.save(&mut attempt, &bytes, |_| {})).unwrap();
    let file = attempt.file.as_ref().unwrap().id;
    let (_, restored) = block_on(client.download(file, revision.id, |_| {})).unwrap();
    assert_eq!(restored, bytes);
    assert_eq!(block_on(client.projects()).unwrap().len(), 1);
    let other = FileClient::with_transport(PocketTransport {
        pic: &pic,
        canister,
        caller: Principal::self_authenticating([32; 32]),
        lose_commit_reply: Cell::new(false),
    });
    assert!(block_on(other.download(file, revision.id, |_| {})).is_err());
    assert!(block_on(other.projects()).unwrap().is_empty());
}
