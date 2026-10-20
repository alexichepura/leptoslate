//! File/media callbacks for the editable surface.

use std::{fmt, rc::Rc};

use web_sys::File;

/// A callback invoked with files pasted or dropped onto the editor.
///
/// The application owns the upload/insertion policy. A typical demo reads the
/// files as data URLs; a production app would upload them and insert the
/// resulting URL.
#[derive(Clone)]
pub struct FileHandler(Rc<dyn Fn(Vec<File>)>);

impl FileHandler {
    pub fn new<F>(handler: F) -> Self
    where
        F: Fn(Vec<File>) + 'static,
    {
        Self(Rc::new(handler))
    }

    pub fn call(&self, files: Vec<File>) {
        (self.0)(files);
    }
}

impl fmt::Debug for FileHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FileHandler")
    }
}
