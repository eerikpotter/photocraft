//! Browser host adapter: the editor retains ownership of its native document format.
use sovereign_cloud_ui::{AppSpec, DocumentState, Editor, Snapshot};

mod menu;
use photocraft_ui_egui::PhotocraftApp;
pub struct Cloud(sovereign_cloud_ui::Cloud);
impl Cloud {
    pub fn new(ctx: egui::Context) -> Self {
        Self(sovereign_cloud_ui::Cloud::new(AppSpec::PHOTO, ctx))
    }
    pub fn install_commands(&self, services: &mut photocraft_ui_egui::Services, _ctx: &egui::Context) {
        let handle = self.0.menu_handle();
        menu::install(services, move |copy| {
            use sovereign_cloud_ui::Request;
            handle.request(match copy {
                None => Request::Open,
                Some(false) => Request::Save,
                Some(true) => Request::SaveAs,
            });
            Ok(())
        });
    }
    pub fn ui(&mut self, ctx: &egui::Context, app: &mut PhotocraftApp) {
        self.0.ui(ctx, &mut Adapter(app));
    }
}
struct Adapter<'a>(&'a mut PhotocraftApp);
impl Editor for Adapter<'_> {
    fn document(&self) -> Option<DocumentState> {
        self.0.session.active().map(|d| DocumentState { id: d.doc.id.0, revision: d.revision, name: d.doc.name.clone(), dirty: d.is_dirty() })
    }
    fn capture(&self) -> Result<Snapshot, String> {
        let document = self.document().ok_or("No image is open")?;
        let doc = self.0.session.active().ok_or("No image is open")?.doc.clone();
        Ok(Snapshot {
            document,
            encode: Box::new(move || photocraft_io::export(&doc, "file.pcraft", &Default::default()).map(|export| export.bytes).map_err(|e| e.to_string())),
        })
    }
    fn saved(&mut self, id: u64, revision: u64, _bytes: &[u8]) -> Result<Option<u64>, String> {
        if let Some(doc) = self.0.session.active_mut().filter(|d| d.doc.id.0 == id && d.revision == revision) {
            doc.saved_revision = revision;
            return Ok(Some(revision));
        }
        Ok(None)
    }
    fn open(&mut self, name: &str, bytes: &[u8]) -> Result<DocumentState, String> {
        self.0.open_file(name, bytes).map_err(|e| e.to_string())?;
        self.document().ok_or_else(|| "Could not open the image".into())
    }
    fn set_save_enabled(&mut self, enabled: bool) {
        for command in &mut self.0.services.commands {
            if matches!(command.id, "host.cloud.save" | "host.cloud.save_copy") {
                command.enabled = enabled;
            }
        }
    }
}
