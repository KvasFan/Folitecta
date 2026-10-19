//! The printers, papers, capabilities, tickets and errors every backend shares.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use folitecta_core::print::Sides;

/// A printer of this computer.
#[derive(Debug, Clone, PartialEq)]
pub struct PrinterInfo {
    /// The name of its queue.
    pub name: String,
    /// The name shown.
    pub label: String,
    pub location: String,
    pub model: String,
    pub is_default: bool,
    pub state: PrinterState,
    /// What the printer reports about its state.
    pub message: String,
    /// Jobs waiting.
    pub jobs: u32,
    pub accepting: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrinterState {
    Idle,
    Printing,
    Stopped,
    Offline,
    Unknown,
}

/// A paper a printer takes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Media {
    /// The PWG media keyword, or `dm:<code>` for a paper of a Windows driver.
    pub key: String,
    /// The short name of a standard paper (A4, Letter), else the driver's name or empty.
    pub name: String,
    /// Upright, in points.
    pub width: f32,
    pub height: f32,
    /// Left, bottom, right and top margins the printer cannot print on, in points.
    pub margins: [f32; 4],
}

impl Media {
    pub fn paper(&self) -> folitecta_core::print::Paper {
        folitecta_core::print::Paper::new(&self.key, self.width, self.height).with_margins(self.margins)
    }
}

/// What a printer can do.
#[derive(Debug, Clone, PartialEq)]
pub struct PrinterCaps {
    pub media: Vec<Media>,
    pub default_media: Option<String>,
    /// The sides it prints (one-sided always).
    pub sides: Vec<Sides>,
    pub duplex: bool,
    pub default_sides: Sides,
    pub max_copies: u32,
    /// It collates copies itself.
    pub collate: bool,
    pub color: bool,
    pub default_color: bool,
    /// The colour modes it names (IPP).
    pub color_modes: Vec<String>,
    /// Resolutions in dots per inch.
    pub resolutions: Vec<(u32, u32)>,
    pub accepts_pdf: bool,
    /// It can be told not to scale (IPP `print-scaling=none`).
    pub scaling_none: bool,
}

impl Default for PrinterCaps {
    fn default() -> Self {
        PrinterCaps {
            media: Vec::new(),
            default_media: None,
            sides: vec![Sides::OneSided],
            duplex: false,
            default_sides: Sides::OneSided,
            max_copies: 1,
            collate: false,
            color: false,
            default_color: false,
            color_modes: Vec::new(),
            resolutions: Vec::new(),
            accepts_pdf: true,
            scaling_none: false,
        }
    }
}

/// How a job prints.
#[derive(Debug, Clone, PartialEq)]
pub struct JobTicket {
    pub printer: String,
    pub job_name: String,
    pub copies: u32,
    pub collate: bool,
    pub sides: Sides,
    /// The paper (`None`: the paper of each sheet, chosen by its size).
    pub media: Option<Media>,
    pub gray: bool,
    /// The resolution of the pictures sent to a printer that receives pixels (Windows).
    #[cfg_attr(unix, allow(dead_code))]
    pub dpi: u32,
    /// The settings of the driver chosen in its own window (Windows).
    #[cfg_attr(unix, allow(dead_code))]
    pub devmode: Option<Vec<u8>>,
}

/// How far a job has come.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JobProgress {
    /// Sheets of the print file written.
    Building { done: usize, total: usize },
    /// Bytes sent to the print server.
    Sending { sent: u64, total: u64 },
    /// Sheets drawn for the printer.
    Rendering { sheet: usize, total: usize },
}

/// A job accepted by the printer.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JobOutcome {
    pub job_id: Option<u32>,
    /// Settings the printer ignored.
    pub ignored: Vec<String>,
}

/// Why printing failed.
#[derive(Debug, Clone, PartialEq)]
pub enum PrintError {
    /// No print server on this computer.
    NoServer,
    /// The print server set for this computer is another computer.
    RemoteServer(String),
    /// The print server asks for a password.
    Auth,
    NotFound(String),
    NotAccepting,
    /// The printer does not take PDF files (a raw queue).
    FormatNotSupported,
    Cancelled,
    Server {
        status: u16,
        message: String,
    },
    Io(String),
    /// The printer driver refused (Windows).
    #[cfg_attr(unix, allow(dead_code))]
    Driver(String),
}

impl fmt::Display for PrintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrintError::NoServer => write!(f, "no print server was found on this computer"),
            PrintError::RemoteServer(h) => write!(f, "the print server {h} is on another computer"),
            PrintError::Auth => write!(f, "the print server asks for a password"),
            PrintError::NotFound(p) => write!(f, "the printer {p} does not exist"),
            PrintError::NotAccepting => write!(f, "the printer is not accepting jobs"),
            PrintError::FormatNotSupported => write!(f, "the printer does not accept PDF files"),
            PrintError::Cancelled => write!(f, "printing was cancelled"),
            PrintError::Server { status, message } => {
                write!(f, "the print server answered 0x{status:04x} {message}")
            }
            PrintError::Io(e) => write!(f, "{e}"),
            PrintError::Driver(e) => write!(f, "the printer driver refused the job: {e}"),
        }
    }
}

impl std::error::Error for PrintError {}

/// The printers of this computer, as one system offers them.
pub trait PrintBackend: Send + Sync {
    /// The printers (quick).
    fn printers(&self) -> Result<Vec<PrinterInfo>, PrintError>;
    /// What a printer can do (may take a while: call away from the interface).
    fn capabilities(&self, printer: &str) -> Result<PrinterCaps, PrintError>;
    /// Send a checked print file. `progress` reports how far it has come;
    /// `cancel` stops the job before it is complete.
    fn submit(
        &self,
        ticket: &JobTicket,
        pdf: Arc<Vec<u8>>,
        progress: &dyn Fn(JobProgress),
        cancel: &AtomicBool,
    ) -> Result<JobOutcome, PrintError>;
    /// The printer receives pixels: the sheets are drawn here (Windows).
    fn raster(&self) -> bool {
        false
    }
}
