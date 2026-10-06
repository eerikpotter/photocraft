//! Optional browser/cloud adapter. No changes to the upstream engine or document format.
mod api;
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
    Initialized,
    SignedIn,
    SignedOut,
    Error(String),
    Progress(String),
    Projects(Vec<Project>),
    Prepared(Attempt),
    Saved(Attempt, Revision),
    Opened(Project, Revision, Vec<u8>),
    Removed,
}
pub struct Cloud {
    auth: Rc<RefCell<Option<AuthClient>>>,
    tx: mpsc::Sender<Event>,
    rx: mpsc::Receiver<Event>,
    visible: bool,
    busy: bool,
    signing_in: bool,
    status: String,
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
            visible: true,
            busy: false,
            signing_in: false,
            status: "Preparing cloud sign-in…".into(),
            name: "Untitled project".into(),
            projects: vec![],
            bindings: BTreeMap::new(),
            attempt: None,
            delete: None,
        }
    }
    fn principal(&self) -> Option<Principal> {
        self.auth.borrow().as_ref().filter(|a| a.is_authenticated()).and_then(|a| a.principal().ok())
    }
    fn api(&self) -> CloudResult<Api> {
        Api::new(self.auth.borrow().as_ref().ok_or("Identity is still loading")?)
    }
    fn refresh(&mut self, ctx: &egui::Context) {
        let api = match self.api() {
            Ok(a) => a,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        self.busy = true;
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
        if let Some(auth) = self.auth.borrow().as_ref() {
            self.busy = true;
            self.signing_in = true;
            self.status = "Complete sign-in in the Internet Identity window…".into();
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
    fn save(&mut self, app: &PhotocraftApp, ctx: &egui::Context, new_project: bool) {
        let Some(doc) = app.session.active() else {
            self.status = "Open or create a document first".into();
            return;
        };
        let binding = self.bindings.get(&doc.doc.id.0).filter(|(owner, _, _)| Some(*owner) == self.principal());
        let project = if new_project { None } else { binding.and_then(|(_, id, _)| self.projects.iter().find(|p| p.id == *id)).cloned() };
        if !new_project && binding.is_some() && project.is_none() {
            self.status = "The linked cloud project is unavailable. Refresh projects or use Save as new.".into();
            return;
        }
        let expected = if new_project { None } else { binding.map(|(_, _, rev)| *rev) };
        match photocraft_io::export(&doc.doc, "project.pcraft", &Default::default()) {
            Ok(export) => {
                if export.bytes.len() as u64 > MAX_DOCUMENT_BYTES {
                    self.status = "This demo supports documents up to 64 MiB encoded as .pcraft".into();
                    return;
                }
                let attempt = Attempt {
                    project,
                    upload: None,
                    expected,
                    bytes: Arc::new(export.bytes),
                    name: self.name.clone(),
                    request: format!("{}-{}-{}", js_sys::Date::now(), doc.doc.id.0, doc.revision),
                    doc_id: doc.doc.id.0,
                    doc_revision: doc.revision,
                };
                self.attempt = Some(attempt.clone());
                self.run_save(attempt, ctx);
            }
            Err(e) => self.status = format!("Could not encode project: {e}"),
        }
    }
    fn run_save(&mut self, mut attempt: Attempt, ctx: &egui::Context) {
        let api = match self.api() {
            Ok(a) => a,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        self.busy = true;
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        wasm_bindgen_futures::spawn_local(async move {
            let result: CloudResult<Revision> = async {
                send(&tx, &ctx, Event::Progress("Preparing cloud revision…".into()));
                if attempt.project.is_none() {
                    attempt.project = Some(api.update("create_project", args((attempt.name.clone(),))?).await?);
                    send(&tx, &ctx, Event::Prepared(attempt.clone()));
                }
                let project = attempt.project.as_ref().ok_or("Missing cloud project")?;
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
                    send(&tx, &ctx, Event::Progress(format!("Saving chunk {} of {count}…", index + 1)));
                    api.update::<()>("put_chunk", args((id, index as u64, chunk.to_vec()))?).await?;
                }
                send(&tx, &ctx, Event::Progress("Verifying and committing revision…".into()));
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
                self.status = e;
                return;
            }
        };
        self.busy = true;
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        wasm_bindgen_futures::spawn_local(async move {
            let result: CloudResult<(Revision, Vec<u8>)> = async {
                let revision: Revision = api.update("get_revision", args((project.id, revision.id))?).await?;
                if revision.bytes > MAX_DOCUMENT_BYTES || revision.chunk_count != revision.bytes.div_ceil(CHUNK_BYTES as u64) {
                    return Err("Invalid project manifest".into());
                }
                let mut bytes = Vec::new();
                for index in 0..revision.chunk_count {
                    send(&tx, &ctx, Event::Progress(format!("Opening chunk {} of {}…", index + 1, revision.chunk_count)));
                    let chunk = api.chunk(project.id, revision.id, index).await?;
                    if chunk.len() > CHUNK_BYTES || bytes.len() as u64 + chunk.len() as u64 > revision.bytes {
                        return Err("Invalid project chunk".into());
                    }
                    bytes.extend(chunk);
                }
                if bytes.len() as u64 != revision.bytes || Sha256::digest(&bytes).to_vec() != revision.sha256 {
                    return Err("Project checksum verification failed".into());
                }
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
                self.status = e;
                return;
            }
        };
        self.busy = true;
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        wasm_bindgen_futures::spawn_local(async move {
            let result = match args((id,)) {
                Ok(a) => api.update::<()>(method, a).await,
                Err(e) => Err(e),
            };
            send(
                &tx,
                &ctx,
                match result {
                    Ok(()) => Event::Removed,
                    Err(e) => Event::Error(e),
                },
            );
        });
    }
    fn poll(&mut self, app: &mut PhotocraftApp, ctx: &egui::Context) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Initialized | Event::SignedIn => {
                    self.busy = false;
                    self.signing_in = false;
                    self.status = "Cloud projects are optional. Editing stays on this device.".into();
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
                    self.status = "Signed out. Your open document stays on this device.".into();
                }
                Event::Error(e) => {
                    self.busy = false;
                    self.signing_in = false;
                    self.status = e;
                }
                Event::Progress(s) => self.status = s,
                Event::Projects(p) => {
                    self.projects = p;
                    self.busy = false;
                }
                Event::Prepared(a) => self.attempt = Some(a),
                Event::Saved(a, r) => {
                    self.busy = false;
                    self.attempt = None;
                    if let Some(p) = a.project {
                        self.bindings.insert(a.doc_id, (p.owner, p.id, r.id));
                        // Only the snapshot that reached consensus is saved. Edits made during upload stay dirty.
                        if let Some(doc) = app.session.active_mut().filter(|d| d.doc.id.0 == a.doc_id && d.revision == a.doc_revision) {
                            doc.saved_revision = a.doc_revision;
                        }
                        self.status = format!("Saved to sovereign cloud · revision {} · {} KiB", r.id, r.bytes.div_ceil(1024));
                    }
                    self.refresh(ctx);
                }
                Event::Opened(p, r, bytes) => {
                    self.busy = false;
                    match app.open_file(&format!("{}.pcraft", p.name), &bytes) {
                        Ok(_) => {
                            if let Some(doc) = app.session.active() {
                                self.bindings.insert(doc.doc.id.0, (p.owner, p.id, r.id));
                            }
                            self.name = p.name;
                            self.status = format!("Opened verified cloud revision {}", r.id);
                        }
                        Err(e) => self.status = format!("Cloud bytes verified; document import failed: {e}"),
                    }
                }
                Event::Removed => {
                    self.busy = false;
                    self.attempt = None;
                    self.status = "Cloud item removed".into();
                    self.refresh(ctx);
                }
            }
        }
    }
    pub fn ui(&mut self, ctx: &egui::Context, app: &mut PhotocraftApp) {
        self.poll(app, ctx);
        egui::Area::new(egui::Id::new("photocraft.cloud.launcher")).anchor(egui::Align2::RIGHT_TOP, [-18.0, 36.0]).order(egui::Order::Foreground).show(
            ctx,
            |ui| {
                let logo = egui::Image::new(egui::include_image!("../../assets/icp/logo.svg")).fit_to_exact_size(egui::vec2(28.0, 16.0));
                if ui
                    .add(egui::Button::image_and_text(logo, "Sovereign Cloud · Save / Open"))
                    .on_hover_text("Save and open ICP cloud projects. File → Save downloads a local copy.")
                    .clicked()
                {
                    self.visible = !self.visible;
                }
            },
        );
        let mut visible = self.visible;
        egui::Window::new("PhotoCraft · Sovereign Cloud").id(egui::Id::new("photocraft.cloud.window")).open(&mut visible).default_width(340.0).default_pos([700.0,80.0]).show(ctx, |ui| {
            ui.label("Local compute. Network-native state.");
            ui.separator();
            if let Some(principal) = self.principal() {
                ui.label("Signed in with Internet Identity");
                ui.small(principal.to_text());
                ui.add_enabled_ui(!self.busy, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Refresh projects").clicked() { self.refresh(ctx); }
                        if ui.button("Sign out").clicked() {
                            let auth = self.auth.borrow().clone();
                            let (tx, ctx) = (self.tx.clone(), ctx.clone());
                            self.busy = true;
                            wasm_bindgen_futures::spawn_local(async move { if let Some(a) = auth { a.logout(None).await; } send(&tx, &ctx, Event::SignedOut); });
                        }
                    });
                    ui.separator();
                    ui.label("Save the active document to your ICP account");
                    ui.label("Project name"); ui.text_edit_singleline(&mut self.name);
                    ui.horizontal(|ui| {
                        if ui.add_enabled(app.session.active().is_some() && self.attempt.is_none(), egui::Button::new("Save to sovereign cloud")).clicked() { self.save(app, ctx, false); }
                        if ui.add_enabled(app.session.active().is_some() && self.attempt.is_none(), egui::Button::new("Save as new")).clicked() { self.save(app, ctx, true); }
                    });
                    if app.session.active().is_none() { ui.small("Create or open a document to enable cloud saving."); }
                    ui.small("File → Save / Export downloads to this device. Use the button above to save to cloud.");
                    if let Some(a) = self.attempt.clone() {
                        if ui.button("Retry this save").clicked() { self.run_save(a, ctx); }
                        if ui.button("Forget failed save (keeps document)").clicked() { self.attempt = None; self.refresh(ctx); }
                    }
                    egui::ScrollArea::vertical().max_height(250.0).show(ui, |ui| {
                        for p in self.projects.clone() {
                            ui.push_id(p.id, |ui| {
                                ui.separator(); ui.strong(&p.name);
                                if let Some(r) = p.revisions.last() && ui.button("Open latest").clicked() { self.open(p.clone(), r.clone(), ctx); }
                                for r in p.revisions.iter().rev() {
                                    ui.horizontal(|ui| {
                                        if ui.button(format!("Open revision {}", r.id)).clicked() { self.open(p.clone(), r.clone(), ctx); }
                                        ui.small(format!("{} KiB", r.bytes.div_ceil(1024)));
                                        if ui.small_button("Delete").clicked() { self.delete = Some((r.id, "delete_revision_or_upload".into())); }
                                    });
                                }
                                if let Some(id) = p.pending_upload && ui.button("Discard unfinished upload").clicked() { self.delete = Some((id, "delete_revision_or_upload".into())); }
                                if p.revisions.is_empty() && p.pending_upload.is_none() && ui.button("Delete empty project").clicked() { self.delete = Some((p.id, "delete_project".into())); }
                            });
                        }
                    });
                });
            } else {
                ui.label("Sign in to save projects and open them on another device.");
                let label = if self.signing_in { "Reopen Internet Identity" } else { "Sign in with Internet Identity" };
                if ui.add_enabled((!self.busy || self.signing_in) && self.auth.borrow().is_some(), egui::Button::new(label)).clicked() { self.login(ctx); }
                if self.signing_in { ui.small("No window? Allow this site's sign-in popup, or open the app in your regular browser."); }
            }
            ui.separator();
            if self.busy { ui.spinner(); ctx.request_repaint_after(std::time::Duration::from_millis(100)); }
            ui.label(&self.status);
            ui.collapsing("Architecture and demo limits", |ui| {
                ui.label("The editor runs on this device. ICP delivers the app and stores owner-only project revisions. No conventional application server or object store is used.");
                ui.label("Save is explicit. Refreshing before a successful save can lose changes. Downloads and local editing remain available without sign-in.");
                ui.label("64 MiB per document; 256 MiB per account including versions; 20 projects and 20 revisions per project. Total service storage is capped at 2 GiB.");
                ui.label("Documents are access-controlled, not end-to-end encrypted. The service controller can upgrade the backend. Sharing, collaboration and agent authorization are future phases.");
                ui.hyperlink_to("Source and architecture", "https://github.com/eerikpotter/photocraft/blob/main/docs/ICP_CLOUD_ARCHITECTURE.md");
            });
        });
        self.visible = visible;
        if let Some((id, method)) = self.delete.clone() {
            egui::Window::new("Delete cloud data?").collapsible(false).show(ctx, |ui| {
                ui.label("This permanently removes the selected cloud item. Open local documents are unaffected.");
                if ui.button("Delete permanently").clicked() {
                    self.delete = None;
                    self.remove(id, if method == "delete_project" { "delete_project" } else { "delete_revision_or_upload" }, ctx);
                }
                if ui.button("Cancel").clicked() {
                    self.delete = None;
                }
            });
        }
    }
}
