//! Rust IC client. Identity protocol: II's documented authorize-client delegation exchange.
use candid::{CandidType, Principal, decode_one, encode_args};
use ic_agent::Agent;
use ic_auth_client::AuthClient;
use photocraft_cloud_protocol::*;
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;
use wasm_bindgen::JsCast;

#[derive(Clone)]
pub struct Api {
    agent: Agent,
    canister: Principal,
}

/// The static-site canister supplies this cookie. Reject contradictory copies (partitioned cookies).
fn environment() -> CloudResult<BTreeMap<String, String>> {
    let document = web_sys::window().and_then(|w| w.document()).ok_or("Browser document unavailable")?;
    let html = document.dyn_into::<web_sys::HtmlDocument>().map_err(|_| "HTML document unavailable")?;
    let cookies = html.cookie().map_err(|_| "Browser cookies unavailable")?;
    let mut found = None;
    for cookie in cookies.split(';').map(str::trim).filter_map(|c| c.strip_prefix("ic_env=")) {
        let decoded = percent_encoding::percent_decode_str(cookie).decode_utf8().map_err(|_| "Invalid cloud environment cookie")?;
        let env: BTreeMap<_, _> = decoded.split('&').filter_map(|v| v.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect();
        if found.as_ref().is_some_and(|old| old != &env) {
            return Err("Conflicting cloud environment cookies; clear site cookies and reload".into());
        }
        found = Some(env);
    }
    found.ok_or("Cloud environment unavailable. Deploy both frontend and cloud canisters with icp deploy.".into())
}

impl Api {
    pub fn new(auth: &AuthClient) -> CloudResult<Self> {
        if !auth.is_authenticated() {
            return Err("Sign in to Internet Identity first".into());
        }
        let env = environment()?;
        let canister = Principal::from_text(env.get("PUBLIC_CANISTER_ID:cloud").ok_or("Cloud canister is not configured")?).map_err(|e| e.to_string())?;
        let origin = web_sys::window().ok_or("Browser unavailable")?.location().origin().map_err(|_| "Cannot read application origin")?;
        let agent = Agent::builder().with_url(origin).with_arc_identity(auth.identity()).build().map_err(|e| e.to_string())?;
        let key = hex::decode(env.get("ic_root_key").ok_or("Missing network trust key")?).map_err(|_| "Invalid network trust key")?;
        if key.len() != 133 {
            return Err("Invalid network trust key length".into());
        }
        agent.set_root_key(key);
        Ok(Self { agent, canister })
    }
    pub async fn update<T: CandidType + DeserializeOwned>(&self, method: &str, args: Vec<u8>) -> CloudResult<T> {
        let bytes = self.agent.update(&self.canister, method).with_arg(args).call_and_wait().await.map_err(|e| e.to_string())?;
        decode_one::<CloudResult<T>>(&bytes).map_err(|e| e.to_string())?
    }
    pub async fn projects(&self) -> CloudResult<Vec<Project>> {
        self.update("list_projects", args(())?).await
    }
    pub async fn chunk(&self, project: u64, revision: u64, index: u64) -> CloudResult<Vec<u8>> {
        let bytes = self.agent.query(&self.canister, "get_chunk").with_arg(args((project, revision, index))?).call().await.map_err(|e| e.to_string())?;
        decode_one::<CloudResult<Vec<u8>>>(&bytes).map_err(|e| e.to_string())?
    }
}
pub fn args<T: candid::utils::ArgumentEncoder>(args: T) -> CloudResult<Vec<u8>> {
    encode_args(args).map_err(|e| e.to_string())
}
