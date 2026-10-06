//! Optional browser/cloud adapter. No changes to the upstream engine or document format.
mod api;
mod panel;
mod presentation;
use api::{Api, args};
use candid::Principal;
use ic_auth_client::{AuthClient, AuthClientLoginOptions};
use photocraft_cloud_protocol::*;
use photocraft_ui_egui::PhotocraftApp;
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    sync::{Arc, mpsc},
};

#[derive(Clone)]
struct Attempt {
    project: Option<Project>,
    upload: Option<u64>,
    expected: Option<u64>,
    bytes: Arc<Vec<u8>>,
    name: String,
    request: String,
    doc_id: u64,
    doc_revision: u64,
}
enum Event {
    RequestSave,
    RequestProjects,
    Initialized,
    SignedIn,
    SignedOut,
    Error(String),
    Progress(String, Option<f32>),
    Projects(Vec<Project>),
    Prepared(Attempt),
    Saved(Attempt, Revision),
    Opened(Project, Revision, Vec<u8>),
    Removed { file: Option<u64>, item: u64 },
}
#[derive(Clone, Copy, PartialEq)]
enum View {
    Save,
    Projects,
}
struct PendingSave {
    document: Arc<photocraft_doc::Document>,
    attempt: Attempt,
    ready_frame: u64,
}
struct Notice {
    message: String,
    error: bool,
    expires: Option<f64>,
}
pub struct Cloud {
    auth: Rc<RefCell<Option<AuthClient>>>,
    tx: mpsc::Sender<Event>,
    rx: mpsc::Receiver<Event>,
    visible: bool,
    view: View,
    busy: bool,
    signing_in: bool,
    status: String,
    progress: Option<f32>,
    notice: Option<Notice>,
    pending_save: Option<PendingSave>,
    copy_mode: bool,
    named_doc: Option<u64>,
    name: String,
    projects: Vec<Project>,
    bindings: BTreeMap<u64, (Principal, u64, u64)>,
    attempt: Option<Attempt>,
    delete: Option<(u64, String)>,
}
fn send(tx: &mpsc::Sender<Event>, ctx: &egui::Context, event: Event) {
    let _ = tx.send(event);
    ctx.request_repaint();
}

impl Cloud {
    pub fn new(ctx: egui::Context) -> Self {
        let (tx, rx) = mpsc::channel();
        let auth = Rc::new(RefCell::new(None));
        let (slot, sender) = (auth.clone(), tx.clone());
        wasm_bindgen_futures::spawn_local(async move {
            // Never reload on inactivity: that would discard unsaved editor work.
            match AuthClient::builder().disable_idle(true).build().await {
                Ok(client) => {
                    *slot.borrow_mut() = Some(client);
                    send(&sender, &ctx, Event::Initialized);
                }
                Err(e) => send(&sender, &ctx, Event::Error(format!("Identity initialization: {e}"))),
            }
        });
        Self {
            auth,
            tx,
            rx,
            visible: false,
            view: View::Projects,
            busy: false,
            signing_in: false,
            status: String::new(),
            progress: None,
            notice: None,
            pending_save: None,
            copy_mode: false,
            named_doc: None,
            name: "Untitled".into(),
            projects: vec![],
            bindings: BTreeMap::new(),
            attempt: None,
            delete: None,
        }
    }
    pub fn install_commands(&self, services: &mut photocraft_ui_egui::Services, ctx: &egui::Context) {
        for (id, label, after, save) in
            [("host.cloud.save", "Save to Cloud…", "file.saveAs", true), ("host.cloud.projects", "Open from Cloud…", "host.cloud.save", false)]
        {
            let (tx, ctx) = (self.tx.clone(), ctx.clone());
            services.commands.push(photocraft_ui_egui::service_commands::Command {
                id,
                label,
                path: &["File"],
                after,
                requires_document: save,
                enabled: true,
                request: Box::new(move || {
                    tx.send(if save { Event::RequestSave } else { Event::RequestProjects }).map_err(|_| "Cloud service is unavailable".to_string())?;
                    ctx.request_repaint();
                    Ok(())
                }),
            });
        }
    }
    fn principal(&self) -> Option<Principal> {
        self.auth.borrow().as_ref().filter(|a| a.is_authenticated()).and_then(|a| a.principal().ok())
    }
    fn api(&self) -> CloudResult<Api> {
        Api::new(self.auth.borrow().as_ref().ok_or("Identity is still loading")?)
    }
    fn begin_operation(&mut self, message: impl Into<String>) {
        self.busy = true;
        self.status = message.into();
        self.progress = None;
        self.notice = None;
    }
    fn notify(&mut self, message: impl Into<String>, error: bool) {
        self.notice = Some(Notice { message: message.into(), error, expires: if error { None } else { Some(js_sys::Date::now() + 8000.0) } });
    }
    fn refresh(&mut self, ctx: &egui::Context) {
        let api = match self.api() {
            Ok(a) => a,
            Err(e) => {
                self.notify(e, true);
                return;
            }
        };
        self.begin_operation("Loading your files…");
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        wasm_bindgen_futures::spawn_local(async move {
            send(
                &tx,
                &ctx,
                match api.projects().await {
                    Ok(p) => Event::Projects(p),
                    Err(e) => Event::Error(e),
                },
            );
        });
    }
    fn login(&mut self, ctx: &egui::Context) {
        let (success_tx, error_tx, success_ctx, error_ctx) = (self.tx.clone(), self.tx.clone(), ctx.clone(), ctx.clone());
        let auth = self.auth.borrow().clone();
        if let Some(auth) = auth {
            self.begin_operation("Complete sign-in in the Internet Identity window…");
            self.signing_in = true;
            auth.login_with_options(
                AuthClientLoginOptions::builder()
                    .identity_provider("https://id.ai")
                    .max_time_to_live(8 * 60 * 60 * 1_000_000_000)
                    .on_success(move |_| send(&success_tx, &success_ctx, Event::SignedIn))
                    .on_error(move |e: Option<String>| send(&error_tx, &error_ctx, Event::Error(e.unwrap_or_else(|| "Sign-in cancelled".into()))))
                    .build(),
            );
        }
    }
    fn save(&mut self, app: &PhotocraftApp, ctx: &egui::Context, copy: bool) {
        if self.busy || self.attempt.is_some() {
            return;
        }
        let Some(doc) = app.session.active() else {
            self.notify("Open or create a document first", true);
            return;
        };
        let binding = self.bindings.get(&doc.doc.id.0).filter(|(owner, _, _)| Some(*owner) == self.principal());
        let project = if copy { None } else { binding.and_then(|(_, id, _)| self.projects.iter().find(|p| p.id == *id)).cloned() };
        if !copy && binding.is_some() && project.is_none() {
            self.notify("This cloud file is unavailable. Refresh My files, or choose More → Save a copy.", true);
            return;
        }
        let name = project.as_ref().map_or_else(|| self.name.trim().to_string(), |p| p.name.clone());
        if name.is_empty() || name.len() > 160 {
            self.notify("Enter a file name of 1–160 bytes.", true);
            return;
        }
        let attempt = Attempt {
            project,
            upload: None,
            expected: if copy { None } else { binding.map(|(_, _, rev)| *rev) },
            bytes: Arc::new(Vec::new()),
            name,
            request: format!("{}-{}-{}", js_sys::Date::now(), doc.doc.id.0, doc.revision),
            doc_id: doc.doc.id.0,
            doc_revision: doc.revision,
        };
        // Capture the immutable document now; paint feedback before synchronous encoding.
        self.pending_save = Some(PendingSave { document: doc.doc.clone(), attempt, ready_frame: ctx.cumulative_frame_nr().saturating_add(2) });
        self.begin_operation("Preparing your file…");
        ctx.request_repaint();
    }
    fn prepare_save(&mut self, ctx: &egui::Context) {
        if let Some(pending) = &self.pending_save {
            if ctx.cumulative_frame_nr() < pending.ready_frame {
                ctx.request_repaint();
                return;
            }
        } else {
            return;
        }
        let Some(mut pending) = self.pending_save.take() else {
            return;
        };
        match photocraft_io::export(&pending.document, "file.pcraft", &Default::default()) {
            Ok(export) if export.bytes.len() as u64 <= MAX_DOCUMENT_BYTES => {
                pending.attempt.bytes = Arc::new(export.bytes);
                self.attempt = Some(pending.attempt.clone());
                self.run_save(pending.attempt, ctx);
            }
            result => {
                self.busy = false;
                self.notify(
                    match result {
                        Ok(_) => "This demo supports files up to 64 MiB encoded as .pcraft".into(),
                        Err(e) => format!("Could not prepare your file: {e}"),
                    },
                    true,
                );
            }
        }
    }
    fn run_save(&mut self, mut attempt: Attempt, ctx: &egui::Context) {
        let api = match self.api() {
            Ok(a) => a,
            Err(e) => {
                self.busy = false;
                self.notify(e, true);
                return;
            }
        };
        self.begin_operation("Connecting to your cloud…");
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        wasm_bindgen_futures::spawn_local(async move {
            let result: CloudResult<Revision> = async {
                send(&tx, &ctx, Event::Progress("Preparing cloud storage…".into(), None));
                if attempt.project.is_none() {
                    attempt.project = Some(api.update("create_project", args((attempt.name.clone(),))?).await?);
                    send(&tx, &ctx, Event::Prepared(attempt.clone()));
                }
                let project = attempt.project.as_ref().ok_or("Cloud file is unavailable")?;
                if let Some(id) = attempt.upload {
                    // A lost commit response is safe to retry. An incomplete upload can resume.
                    match api.update::<Revision>("commit_upload", args((id,))?).await {
                        Ok(revision) => return Ok(revision),
                        Err(e) if e == "Upload incomplete" => {}
                        Err(e) => return Err(e),
                    }
                } else {
                    let request = BeginUpload {
                        project_id: project.id,
                        expected_revision: attempt.expected,
                        bytes: attempt.bytes.len() as u64,
                        sha256: Sha256::digest(attempt.bytes.as_ref()).to_vec(),
                        request_id: attempt.request.clone(),
                    };
                    let upload: Upload = api.update("begin_upload", args((request,))?).await?;
                    attempt.upload = Some(upload.id);
                    send(&tx, &ctx, Event::Prepared(attempt.clone()));
                }
                let id = attempt.upload.ok_or("Missing upload ID")?;
                let count = attempt.bytes.len().div_ceil(CHUNK_BYTES);
                for (index, chunk) in attempt.bytes.chunks(CHUNK_BYTES).enumerate() {
                    send(&tx, &ctx, Event::Progress("Uploading your file…".into(), Some(index as f32 / count as f32)));
                    api.update::<()>("put_chunk", args((id, index as u64, chunk.to_vec()))?).await?;
                    send(&tx, &ctx, Event::Progress("Uploading your file…".into(), Some((index + 1) as f32 / count as f32)));
                }
                send(&tx, &ctx, Event::Progress("Finishing save…".into(), None));
                api.update("commit_upload", args((id,))?).await
            }
            .await;
            send(
                &tx,
                &ctx,
                match result {
                    Ok(r) => Event::Saved(attempt, r),
                    Err(e) => Event::Error(e),
                },
            );
        });
    }
    fn open(&mut self, project: Project, revision: Revision, ctx: &egui::Context) {
        let api = match self.api() {
            Ok(a) => a,
            Err(e) => {
                self.busy = false;
                self.notify(e, true);
                return;
            }
        };
        self.begin_operation(format!("Opening {}…", presentation::file_name(&project.name)));
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        wasm_bindgen_futures::spawn_local(async move {
            let result: CloudResult<(Revision, Vec<u8>)> = async {
                let revision: Revision = api.update("get_revision", args((project.id, revision.id))?).await?;
                if revision.bytes > MAX_DOCUMENT_BYTES || revision.chunk_count != revision.bytes.div_ceil(CHUNK_BYTES as u64) {
                    return Err("Invalid file manifest".into());
                }
                let mut bytes = Vec::new();
                for index in 0..revision.chunk_count {
                    send(&tx, &ctx, Event::Progress("Downloading your file…".into(), Some(index as f32 / revision.chunk_count as f32)));
                    let chunk = api.chunk(project.id, revision.id, index).await?;
                    if chunk.len() > CHUNK_BYTES || bytes.len() as u64 + chunk.len() as u64 > revision.bytes {
                        return Err("Invalid file chunk".into());
                    }
                    bytes.extend(chunk);
                }
                if bytes.len() as u64 != revision.bytes || Sha256::digest(&bytes).to_vec() != revision.sha256 {
                    return Err("File integrity check failed".into());
                }
                send(&tx, &ctx, Event::Progress("Opening your file in the editor…".into(), None));
                Ok((revision, bytes))
            }
            .await;
            send(
                &tx,
                &ctx,
                match result {
                    Ok((r, bytes)) => Event::Opened(project, r, bytes),
                    Err(e) => Event::Error(e),
                },
            );
        });
    }
    fn remove(&mut self, id: u64, method: &'static str, ctx: &egui::Context) {
        let api = match self.api() {
            Ok(a) => a,
            Err(e) => {
                self.notify(e, true);
                return;
            }
        };
        // Discarding an initial upload also removes its empty file record in one action.
        let empty_file = self.projects.iter().find(|p| p.pending_upload == Some(id) && p.revisions.is_empty()).map(|p| p.id);
        let file = if method == "delete_project" { Some(id) } else { empty_file };
        self.begin_operation("Removing cloud data…");
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        wasm_bindgen_futures::spawn_local(async move {
            let result: CloudResult<()> = async {
                api.update::<()>(method, args((id,))?).await?;
                if method != "delete_project"
                    && let Some(file) = empty_file
                {
                    api.update::<()>("delete_project", args((file,))?).await?;
                }
                Ok(())
            }
            .await;
            send(
                &tx,
                &ctx,
                match result {
                    Ok(()) => Event::Removed { file, item: id },
                    Err(e) => Event::Error(e),
                },
            );
        });
    }
    fn poll(&mut self, app: &mut PhotocraftApp, ctx: &egui::Context) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::RequestSave => {
                    self.visible = true;
                    self.view = View::Save;
                    self.copy_mode = false;
                    if !self.busy && self.attempt.is_none() {
                        let linked =
                            app.session.active().and_then(|d| self.bindings.get(&d.doc.id.0)).is_some_and(|(owner, _, _)| Some(*owner) == self.principal());
                        if linked {
                            self.save(app, ctx, false);
                        } else if let Some(doc) = app.session.active() {
                            self.name = doc.doc.name.clone();
                            self.named_doc = Some(doc.doc.id.0);
                        }
                    }
                }
                Event::RequestProjects => {
                    self.visible = true;
                    self.view = View::Projects;
                    if !self.busy && self.principal().is_some() {
                        self.refresh(ctx);
                    }
                }
                Event::Initialized | Event::SignedIn => {
                    self.busy = false;
                    self.signing_in = false;
                    self.notice = None;
                    if self.principal().is_some() {
                        self.refresh(ctx);
                    }
                }
                Event::SignedOut => {
                    self.busy = false;
                    self.signing_in = false;
                    self.projects.clear();
                    self.bindings.clear();
                    self.attempt = None;
                    self.copy_mode = false;
                    self.notify("Signed out. Your open files stay in the editor.", false);
                }
                Event::Error(e) => {
                    self.busy = false;
                    self.signing_in = false;
                    self.notify(e.replace("Project", "File").replace("project", "file"), true);
                }
                Event::Progress(s, fraction) => {
                    self.status = s;
                    self.progress = fraction;
                }
                Event::Projects(p) => {
                    self.projects = p;
                    self.busy = false;
                }
                Event::Prepared(a) => self.attempt = Some(a),
                Event::Saved(a, r) => {
                    self.busy = false;
                    self.attempt = None;
                    self.copy_mode = false;
                    if let Some(mut p) = a.project {
                        self.bindings.insert(a.doc_id, (p.owner, p.id, r.id));
                        // Only the snapshot that reached consensus is saved. Edits made during upload stay dirty.
                        if let Some(doc) = app.session.active_mut().filter(|d| d.doc.id.0 == a.doc_id && d.revision == a.doc_revision) {
                            doc.saved_revision = a.doc_revision;
                        }
                        self.notify(format!("Saved {} · {}", presentation::file_name(&p.name), timestamp(r.created_at)), false);
                        p.pending_upload = None;
                        p.revisions.retain(|old| old.id != r.id);
                        p.revisions.push(r);
                        self.projects.retain(|old| old.id != p.id);
                        self.projects.push(p);
                    }
                }
                Event::Opened(p, r, bytes) => {
                    self.busy = false;
                    match app.open_file(&presentation::file_name(&p.name), &bytes) {
                        Ok(_) => {
                            if let Some(doc) = app.session.active() {
                                self.bindings.insert(doc.doc.id.0, (p.owner, p.id, r.id));
                            }
                            self.copy_mode = false;
                            self.notify(format!("Opened {} · saved {}", presentation::file_name(&p.name), timestamp(r.created_at)), false);
                        }
                        Err(e) => self.notify(format!("Could not open your file: {e}"), true),
                    }
                }
                Event::Removed { file, item } => {
                    self.busy = false;
                    if self.attempt.as_ref().is_some_and(|a| a.upload == Some(item) || (file.is_some() && a.project.as_ref().map(|p| p.id) == file)) {
                        self.attempt = None;
                    }
                    self.refresh(ctx);
                    self.notify("Cloud item removed", false);
                }
            }
        }
    }
}

/// Canister timestamps are nanoseconds; browser dates use milliseconds and the device timezone.
fn timestamp(nanos: u64) -> String {
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64((nanos / 1_000_000) as f64));
    date.to_locale_string("en-GB", &wasm_bindgen::JsValue::UNDEFINED).into()
}
