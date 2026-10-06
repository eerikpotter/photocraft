//! Stable IDs are a persistent schema: never reuse a memory ID or change its key type.
use candid::Principal;
use ic_stable_structures::{
    Memory, StableBTreeMap, StableCell,
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
};
use photocraft_cloud_protocol::*;
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};

type Vm<M> = VirtualMemory<M>;
pub struct Store<M: Memory> {
    next: StableCell<u64, Vm<M>>,
    total: StableCell<u64, Vm<M>>,
    projects: StableBTreeMap<u64, Vec<u8>, Vm<M>>,
    uploads: StableBTreeMap<u64, Vec<u8>, Vm<M>>,
    chunks: StableBTreeMap<(u64, u64), Vec<u8>, Vm<M>>,
    usage: StableBTreeMap<Principal, u64, Vm<M>>,
}

fn encode<T: Serialize>(v: &T) -> CloudResult<Vec<u8>> {
    serde_json::to_vec(v).map_err(|e| format!("Storage encoding: {e}"))
}
fn decode<T: DeserializeOwned>(v: &[u8]) -> CloudResult<T> {
    serde_json::from_slice(v).map_err(|e| format!("Storage decoding: {e}"))
}
fn authenticated(caller: Principal) -> CloudResult<()> {
    if caller == Principal::anonymous() { Err("Sign in to use cloud projects".into()) } else { Ok(()) }
}

impl<M: Memory> Store<M> {
    pub fn new(memory: M) -> Self {
        let m = MemoryManager::init(memory);
        Self {
            next: StableCell::init(m.get(MemoryId::new(0)), 1),
            total: StableCell::init(m.get(MemoryId::new(1)), 0),
            projects: StableBTreeMap::init(m.get(MemoryId::new(2))),
            uploads: StableBTreeMap::init(m.get(MemoryId::new(3))),
            chunks: StableBTreeMap::init(m.get(MemoryId::new(4))),
            usage: StableBTreeMap::init(m.get(MemoryId::new(5))),
        }
    }
    fn next_id(&self) -> CloudResult<(u64, u64)> {
        let id = *self.next.get();
        Ok((id, id.checked_add(1).ok_or("ID space exhausted")?))
    }
    fn project(&self, caller: Principal, id: u64) -> CloudResult<Project> {
        authenticated(caller)?;
        let p: Project = decode(&self.projects.get(&id).ok_or("Project not found or not permitted")?)?;
        if p.owner != caller {
            return Err("Project not found or not permitted".into());
        }
        Ok(p)
    }
    fn upload(&self, caller: Principal, id: u64) -> CloudResult<(Upload, Project)> {
        authenticated(caller)?;
        let u: Upload = decode(&self.uploads.get(&id).ok_or("Upload not found or not permitted")?)?;
        let p = self.project(caller, u.request.project_id)?;
        Ok((u, p))
    }
    pub fn list(&self, caller: Principal) -> CloudResult<Vec<Project>> {
        authenticated(caller)?;
        // Globally bounded to MAX_PROJECTS; a future schema can add an owner index.
        self.projects
            .iter()
            .map(|entry| decode::<Project>(&entry.value()))
            .filter_map(|r| match r {
                Ok(p) if p.owner == caller => Some(Ok(p)),
                Ok(_) => None,
                Err(e) => Some(Err(e)),
            })
            .collect()
    }
    pub fn create(&mut self, caller: Principal, name: String, now: u64) -> CloudResult<Project> {
        authenticated(caller)?;
        let name = name.trim().to_string();
        if name.is_empty() || name.len() > 160 {
            return Err("Project names must contain 1–160 UTF-8 bytes".into());
        }
        if self.projects.len() >= MAX_PROJECTS || self.list(caller)?.len() >= MAX_USER_PROJECTS {
            return Err("Project quota reached".into());
        }
        let (id, next) = self.next_id()?;
        let p = Project { id, owner: caller, name, created_at: now, revisions: vec![], pending_upload: None };
        let bytes = encode(&p)?;
        self.projects.insert(id, bytes);
        self.next.set(next);
        Ok(p)
    }
    pub fn begin(&mut self, caller: Principal, request: BeginUpload, now: u64) -> CloudResult<Upload> {
        let mut p = self.project(caller, request.project_id)?;
        if request.bytes == 0 || request.bytes > MAX_DOCUMENT_BYTES || request.sha256.len() != 32 || !(16..=80).contains(&request.request_id.len()) {
            return Err("Invalid upload: 1–64 MiB, SHA-256 and a 16–80 byte request ID required".into());
        }
        if let Some(id) = p.pending_upload {
            let (u, _) = self.upload(caller, id)?;
            if u.request.request_id == request.request_id
                && u.request.bytes == request.bytes
                && u.request.sha256 == request.sha256
                && u.request.expected_revision == request.expected_revision
            {
                return Ok(u);
            }
            return Err("An upload is pending; resume or discard it first".into());
        }
        if p.revisions.last().map(|r| r.id) != request.expected_revision {
            return Err("Revision conflict: reopen the latest project before saving".into());
        }
        if p.revisions.len() >= MAX_REVISIONS {
            return Err("Revision limit reached; delete an older revision first".into());
        }
        let used = self.usage.get(&caller).unwrap_or(0).checked_add(request.bytes).ok_or("Quota overflow")?;
        let total = self.total.get().checked_add(request.bytes).ok_or("Quota overflow")?;
        if used > USER_QUOTA_BYTES || total > TOTAL_QUOTA_BYTES {
            return Err("Cloud storage quota reached".into());
        }
        let (id, next) = self.next_id()?;
        let u = Upload { id, request, created_at: now, committed: None };
        p.pending_upload = Some(id);
        let (pb, ub) = (encode(&p)?, encode(&u)?);
        self.projects.insert(p.id, pb);
        self.uploads.insert(id, ub);
        self.usage.insert(caller, used);
        self.total.set(total);
        self.next.set(next);
        Ok(u)
    }
    pub fn put(&mut self, caller: Principal, id: u64, index: u64, bytes: Vec<u8>) -> CloudResult<()> {
        let (u, p) = self.upload(caller, id)?;
        if u.committed.is_some() || p.pending_upload != Some(id) {
            return Err("Upload is not pending".into());
        }
        let offset = index.checked_mul(CHUNK_BYTES as u64).ok_or("Invalid chunk index")?;
        if offset >= u.request.bytes {
            return Err("Invalid chunk index".into());
        }
        let expected = (u.request.bytes - offset).min(CHUNK_BYTES as u64);
        if bytes.len() as u64 != expected {
            return Err("Wrong chunk length".into());
        }
        if let Some(old) = self.chunks.get(&(id, index)) {
            return if old == bytes { Ok(()) } else { Err("Chunk already stored with different bytes".into()) };
        }
        self.chunks.insert((id, index), bytes);
        Ok(())
    }
    pub fn commit(&mut self, caller: Principal, id: u64, now: u64) -> CloudResult<Revision> {
        let (mut u, mut p) = self.upload(caller, id)?;
        if let Some(r) = u.committed {
            return Ok(r);
        }
        if p.pending_upload != Some(id) || p.revisions.last().map(|r| r.id) != u.request.expected_revision {
            return Err("Revision conflict".into());
        }
        let n = u.request.bytes.div_ceil(CHUNK_BYTES as u64);
        let mut hash = Sha256::new();
        for i in 0..n {
            hash.update(self.chunks.get(&(id, i)).ok_or("Upload incomplete")?);
        }
        if hash.finalize().to_vec() != u.request.sha256 {
            return Err("Document checksum mismatch".into());
        }
        let r = Revision { id, bytes: u.request.bytes, sha256: u.request.sha256.clone(), created_at: now, author: caller, chunk_count: n };
        u.committed = Some(r.clone());
        p.revisions.push(r.clone());
        p.pending_upload = None;
        let (pb, ub) = (encode(&p)?, encode(&u)?);
        self.projects.insert(p.id, pb);
        self.uploads.insert(id, ub);
        Ok(r)
    }
    pub fn revision(&self, caller: Principal, project: u64, revision: u64) -> CloudResult<Revision> {
        self.project(caller, project)?.revisions.into_iter().find(|r| r.id == revision).ok_or("Revision not found".into())
    }
    pub fn chunk(&self, caller: Principal, project: u64, revision: u64, index: u64) -> CloudResult<Vec<u8>> {
        let r = self.revision(caller, project, revision)?;
        if index >= r.chunk_count {
            return Err("Invalid chunk index".into());
        }
        self.chunks.get(&(revision, index)).ok_or("Chunk missing".into())
    }
    pub fn remove_upload(&mut self, caller: Principal, id: u64) -> CloudResult<()> {
        let (u, mut p) = self.upload(caller, id)?;
        if u.committed.is_some() && p.pending_upload.is_some() {
            return Err("Finish or discard the pending upload first".into());
        }
        p.revisions.retain(|r| r.id != id);
        if p.pending_upload == Some(id) {
            p.pending_upload = None;
        }
        let used = self.usage.get(&caller).unwrap_or(0).checked_sub(u.request.bytes).ok_or("Invalid usage accounting")?;
        let total = self.total.get().checked_sub(u.request.bytes).ok_or("Invalid total accounting")?;
        let pb = encode(&p)?;
        for i in 0..u.request.bytes.div_ceil(CHUNK_BYTES as u64) {
            self.chunks.remove(&(id, i));
        }
        self.uploads.remove(&id);
        self.projects.insert(p.id, pb);
        if used == 0 {
            self.usage.remove(&caller);
        } else {
            self.usage.insert(caller, used);
        }
        self.total.set(total);
        Ok(())
    }
    pub fn delete_project(&mut self, caller: Principal, id: u64) -> CloudResult<()> {
        let p = self.project(caller, id)?;
        if p.pending_upload.is_some() || !p.revisions.is_empty() {
            return Err("Delete revisions and discard uploads before deleting this project".into());
        }
        self.projects.remove(&id);
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use ic_stable_structures::VectorMemory;
    fn owner(n: u8) -> Principal {
        Principal::self_authenticating([n; 32])
    }
    fn request(project: u64, data: &[u8], expected: Option<u64>) -> BeginUpload {
        BeginUpload {
            project_id: project,
            bytes: data.len() as u64,
            sha256: Sha256::digest(data).to_vec(),
            request_id: "test-request-00001".into(),
            expected_revision: expected,
        }
    }
    #[test]
    fn ownership_is_enforced_for_every_data_path() {
        let mut s = Store::new(VectorMemory::default());
        let p = s.create(owner(1), "Test".into(), 1).unwrap();
        let u = s.begin(owner(1), request(p.id, b"document", None), 2).unwrap();
        for attacker in [owner(2), Principal::anonymous()] {
            assert!(s.begin(attacker, request(p.id, b"x", None), 2).is_err());
            assert!(s.put(attacker, u.id, 0, b"document".to_vec()).is_err());
            assert!(s.commit(attacker, u.id, 3).is_err());
            assert!(s.remove_upload(attacker, u.id).is_err());
            assert!(s.delete_project(attacker, p.id).is_err());
        }
        s.put(owner(1), u.id, 0, b"document".to_vec()).unwrap();
        s.commit(owner(1), u.id, 3).unwrap();
        assert!(s.chunk(owner(2), p.id, u.id, 0).is_err());
        assert!(s.revision(owner(2), p.id, u.id).is_err());
        assert!(s.list(Principal::anonymous()).is_err());
        assert!(s.list(owner(2)).unwrap().is_empty());
    }
    #[test]
    fn incomplete_upload_never_publishes_and_retries_are_idempotent() {
        let mut s = Store::new(VectorMemory::default());
        let p = s.create(owner(1), "Test".into(), 1).unwrap();
        let data = vec![42; CHUNK_BYTES + 17];
        let req = request(p.id, &data, None);
        let u = s.begin(owner(1), req.clone(), 2).unwrap();
        assert_eq!(s.begin(owner(1), req, 2).unwrap().id, u.id);
        assert!(s.commit(owner(1), u.id, 3).is_err());
        assert!(s.list(owner(1)).unwrap()[0].revisions.is_empty());
        for (i, chunk) in data.chunks(CHUNK_BYTES).enumerate() {
            s.put(owner(1), u.id, i as u64, chunk.to_vec()).unwrap();
            s.put(owner(1), u.id, i as u64, chunk.to_vec()).unwrap();
        }
        assert!(s.put(owner(1), u.id, 1, vec![0; 17]).is_err());
        assert!(s.put(owner(1), u.id, u64::MAX, vec![0]).is_err());
        let r = s.commit(owner(1), u.id, 4).unwrap();
        assert_eq!(r, s.commit(owner(1), u.id, 5).unwrap());
        assert!(s.begin(owner(1), request(p.id, b"stale", None), 6).is_err());
        assert_eq!(s.list(owner(1)).unwrap()[0].revisions.len(), 1);
    }
    #[test]
    fn checksums_bounds_and_quotas_are_checked_before_publication() {
        let mut s = Store::new(VectorMemory::default());
        let p = s.create(owner(1), "Test".into(), 1).unwrap();
        let mut req = request(p.id, b"abc", None);
        req.bytes = MAX_DOCUMENT_BYTES + 1;
        assert!(s.begin(owner(1), req, 1).is_err());
        let u = s.begin(owner(1), request(p.id, b"abc", None), 2).unwrap();
        s.put(owner(1), u.id, 0, b"bad".to_vec()).unwrap();
        assert!(s.commit(owner(1), u.id, 3).is_err());
        assert!(s.list(owner(1)).unwrap()[0].revisions.is_empty());
        s.remove_upload(owner(1), u.id).unwrap();
        assert_eq!(*s.total.get(), 0);
        assert!(s.usage.get(&owner(1)).is_none());
        s.usage.insert(owner(1), USER_QUOTA_BYTES);
        assert!(s.begin(owner(1), request(p.id, b"abc", None), 2).is_err());
    }
    #[test]
    fn reopening_stable_memory_keeps_pending_and_committed_data() {
        let memory = VectorMemory::default();
        let mut s = Store::new(memory.clone());
        let p = s.create(owner(1), "Survives".into(), 1).unwrap();
        let u = s.begin(owner(1), request(p.id, b"document", None), 2).unwrap();
        s.put(owner(1), u.id, 0, b"document".to_vec()).unwrap();
        drop(s);
        let mut s = Store::new(memory.clone());
        s.commit(owner(1), u.id, 3).unwrap();
        drop(s);
        let mut s = Store::new(memory);
        assert_eq!(s.chunk(owner(1), p.id, u.id, 0).unwrap(), b"document");
        assert!(s.delete_project(owner(1), p.id).is_err());
        s.remove_upload(owner(1), u.id).unwrap();
        s.delete_project(owner(1), p.id).unwrap();
        assert!(s.list(owner(1)).unwrap().is_empty());
        assert_eq!(*s.total.get(), 0);
    }
}
