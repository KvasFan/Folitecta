//! Convert: export, images, new documents from pictures.

use super::{doc_dialog, handlers, no_doc};
use crate::files;
use crate::state::{AppState, Dialog};

fn export(st: AppState) {
    doc_dialog(st, Dialog::ExportImages);
}

handlers! {
    "export" => export,
    "document.export" => export,
    "document.export.toImages" => export,
    "saveImage" => export,
    "newDocImage" => |st| files::new_from_images(st),
    "document.addImage" => |st| { if !no_doc(st) { files::pick_image_to_place(st) } },
    "tool.selImagePlace" => |st| { if !no_doc(st) { files::pick_image_to_place(st) } },
    "selImagePlace.apply" => |st| { if !no_doc(st) { files::pick_image_to_place(st) } },
}
