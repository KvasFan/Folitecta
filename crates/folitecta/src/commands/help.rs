//! Help menu.

use super::{handlers, open_url};
use crate::state::Dialog;

const REPO: &str = "https://github.com/KvasExpert/Folitecta";

handlers! {
    "help.about" => |st| st.set_dialog(Dialog::About),
    "help.contents" => |st| open_url(st, &format!("{REPO}#readme")),
    "help.homepage" => |st| open_url(st, REPO),
    "help.support" => |st| open_url(st, &format!("{REPO}/discussions")),
    "help.report" => |st| open_url(st, &format!("{REPO}/issues/new")),
    "help.checkForUpdates" => |st| open_url(st, &format!("{REPO}/releases")),
    "help.cmdline" => |st| st.set_dialog(Dialog::CommandLine),
}
