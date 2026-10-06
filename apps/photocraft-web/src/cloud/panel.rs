//! Cloud file management is a browser-host view, separate from the editor workspace.
use super::*;

fn logo() -> egui::Image<'static> {
    egui::Image::new(egui::include_image!("../../assets/icp/logo.svg")).fit_to_exact_size(egui::vec2(28.0, 16.0))
}

impl Cloud {
    pub fn ui(&mut self, ctx: &egui::Context, app: &mut PhotocraftApp) {
        self.poll(app, ctx);
        egui::Area::new(egui::Id::new("photocraft.cloud.launcher")).anchor(egui::Align2::RIGHT_TOP, [-18.0, 36.0]).order(egui::Order::Foreground).show(
            ctx,
            |ui| {
                if ui
                    .add(egui::Button::image_and_text(logo(), "Sovereign Cloud"))
                    .on_hover_text("Manage cloud projects and revisions. Also available from the File menu.")
                    .clicked()
                {
                    self.visible = !self.visible;
                }
            },
        );
        let mut visible = self.visible;
        egui::Window::new("Sovereign Cloud")
            .id(egui::Id::new("photocraft.cloud.window")).open(&mut visible)
            .default_width(460.0).default_pos([620.0, 90.0]).vscroll(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(logo());
                    ui.heading("Your creative workspace");
                });
                ui.weak("Edit on this device. Keep your projects on ICP.");
                ui.add_space(10.0);
                if let Some(principal) = self.principal() {
                    ui.horizontal(|ui| {
                        ui.label("Signed in with Internet Identity");
                        if ui.add_enabled(!self.busy, egui::Button::new("Sign out")).clicked() { self.sign_out(ctx); }
                    });
                    ui.collapsing("Account details", |ui| { ui.small(principal.to_text()); });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.view, View::Save, "Save document");
                        ui.selectable_value(&mut self.view, View::Projects, format!("Cloud projects ({})", self.projects.len()));
                    });
                    ui.separator();
                    ui.add_enabled_ui(!self.busy, |ui| match self.view {
                        View::Save => self.save_view(ui, app, ctx),
                        View::Projects => self.projects_view(ui, ctx),
                    });
                } else {
                    ui.label("Sign in to save projects and open them on another device.");
                    ui.add_space(8.0);
                    let label = if self.signing_in { "Reopen Internet Identity" } else { "Sign in with Internet Identity" };
                    if ui.add_enabled((!self.busy || self.signing_in) && self.auth.borrow().is_some(), egui::Button::new(label).min_size(egui::vec2(0.0, 32.0))).clicked() {
                        self.login(ctx);
                    }
                    if self.signing_in { ui.small("No window? Allow this site's sign-in popup, or open the app in your regular browser."); }
                }
                ui.add_space(8.0);
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if self.busy { ui.spinner(); ctx.request_repaint_after(std::time::Duration::from_millis(100)); }
                        ui.label(&self.status);
                    });
                });
                ui.add_space(6.0);
                ui.collapsing("About this cloud", |ui| {
                    ui.label("ICP hosts the app and stores owner-only project revisions. Editing and rendering run on your device.");
                    ui.label("Cloud saving is explicit. File → Save / Export downloads a local copy. Save before closing or reloading.");
                    ui.label("Demo limits: 64 MiB per document; 256 MiB per account including versions; 20 projects, 20 revisions each; 2 GiB across the service.");
                    ui.label("Documents are access-controlled, not end-to-end encrypted. The service controller can upgrade the backend. Sharing and collaboration are future features.");
                    ui.hyperlink_to("Source and architecture", "https://github.com/eerikpotter/photocraft/blob/codex/icp-local-hosting/docs/ICP_CLOUD_ARCHITECTURE.md");
                });
            });
        self.visible = visible;
        for command in &mut app.services.commands {
            if command.id == "host.cloud.save" {
                command.enabled = !self.busy && self.attempt.is_none();
            }
        }
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

    fn sign_out(&mut self, ctx: &egui::Context) {
        let auth = self.auth.borrow().clone();
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        self.busy = true;
        wasm_bindgen_futures::spawn_local(async move {
            if let Some(a) = auth {
                a.logout(None).await;
            }
            send(&tx, &ctx, Event::SignedOut);
        });
    }

    fn save_view(&mut self, ui: &mut egui::Ui, app: &PhotocraftApp, ctx: &egui::Context) {
        if let Some(doc) = app.session.active() {
            ui.label("Active document");
            ui.strong(&doc.doc.name);
            let has_binding = self.bindings.get(&doc.doc.id.0).is_some_and(|(owner, _, _)| Some(*owner) == self.principal());
            let project = self
                .bindings
                .get(&doc.doc.id.0)
                .filter(|(owner, _, _)| Some(*owner) == self.principal())
                .and_then(|(_, id, _)| self.projects.iter().find(|p| p.id == *id))
                .cloned();
            if let Some(p) = &project {
                ui.weak(format!("Saves a new revision of “{}”.", p.name));
            } else {
                ui.weak("Creates a cloud project from this document.");
            }
            ui.add_space(8.0);
            ui.label(if project.is_some() { "Name for Save as New Project" } else { "Project name" });
            ui.text_edit_singleline(&mut self.name);
            ui.add_space(8.0);
            ui.add_enabled_ui(self.attempt.is_none(), |ui| {
                if ui.add_sized([ui.available_width(), 34.0], egui::Button::image_and_text(logo(), "Save to Cloud")).clicked() {
                    self.save(app, ctx, false);
                }
                if has_binding && ui.button("Save as New Project…").clicked() {
                    self.save(app, ctx, true);
                }
            });
        } else {
            ui.label("No document is open.");
            ui.weak("Create or open a document in PhotoCraft, then choose File → Save to Cloud.");
        }
        ui.add_space(8.0);
        if let Some(a) = self.attempt.clone() {
            ui.group(|ui| {
                ui.label("A previous save needs attention.");
                if ui.button("Retry this save").clicked() {
                    self.run_save(a, ctx);
                }
                if ui.button("Forget failed save (keeps document)").clicked() {
                    self.attempt = None;
                    self.refresh(ctx);
                }
            });
        }
        ui.weak("Wait for the saved-revision confirmation. Further edits need another save.");
    }

    fn projects_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.strong("Your cloud projects");
            if ui.button("Refresh").clicked() {
                self.refresh(ctx);
            }
        });
        if self.projects.is_empty() {
            ui.add_space(12.0);
            ui.label("No cloud projects yet.");
            ui.weak("Use File → Save to Cloud to save your first document.");
        }
        egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
            for p in self.projects.clone() {
                ui.push_id(p.id, |ui| {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.strong(&p.name);
                            if let Some(r) = p.revisions.last()
                                && ui.button("Open latest").clicked()
                            {
                                self.open(p.clone(), r.clone(), ctx);
                            }
                        });
                        ui.weak(format!("{} saved revision(s)", p.revisions.len()));
                        if !p.revisions.is_empty() {
                            ui.collapsing("Revision history", |ui| {
                                for r in p.revisions.iter().rev() {
                                    ui.horizontal(|ui| {
                                        if ui.button(format!("Open revision {}", r.id)).clicked() {
                                            self.open(p.clone(), r.clone(), ctx);
                                        }
                                        ui.small(format!("{} KiB", r.bytes.div_ceil(1024)));
                                        if ui.small_button("Delete").clicked() {
                                            self.delete = Some((r.id, "delete_revision_or_upload".into()));
                                        }
                                    });
                                }
                            });
                        }
                        if let Some(id) = p.pending_upload
                            && ui.button("Discard unfinished upload").clicked()
                        {
                            self.delete = Some((id, "delete_revision_or_upload".into()));
                        }
                        if p.revisions.is_empty() && p.pending_upload.is_none() && ui.button("Delete empty project").clicked() {
                            self.delete = Some((p.id, "delete_project".into()));
                        }
                    });
                    ui.add_space(6.0);
                });
            }
        });
    }
}
