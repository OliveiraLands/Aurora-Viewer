//! Platform file dialog wrapper: uses `rfd` on desktop and returns `None` on
//! Android until a Storage Access Framework picker is wired up.

#[cfg(not(target_os = "android"))]
pub use rfd::FileDialog;

#[cfg(target_os = "android")]
#[derive(Default)]
pub struct FileDialog;

#[cfg(target_os = "android")]
impl FileDialog {
    pub fn new() -> Self {
        Self
    }

    pub fn set_title(self, _title: impl AsRef<str>) -> Self {
        self
    }

    pub fn add_filter(self, _name: impl AsRef<str>, _extensions: &[impl AsRef<str>]) -> Self {
        self
    }

    pub fn set_directory(self, _path: impl AsRef<std::path::Path>) -> Self {
        self
    }

    pub fn set_file_name(self, _file_name: impl AsRef<str>) -> Self {
        self
    }

    pub fn pick_file(self) -> Option<std::path::PathBuf> {
        log::warn!("file_dialog: pick_file is not supported on Android yet");
        None
    }

    pub fn pick_files(self) -> Option<Vec<std::path::PathBuf>> {
        log::warn!("file_dialog: pick_files is not supported on Android yet");
        None
    }

    pub fn pick_folder(self) -> Option<std::path::PathBuf> {
        log::warn!("file_dialog: pick_folder is not supported on Android yet");
        None
    }

    pub fn save_file(self) -> Option<std::path::PathBuf> {
        log::warn!("file_dialog: save_file is not supported on Android yet");
        None
    }
}
