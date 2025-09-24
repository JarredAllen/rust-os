//! A logging implementation

use core::fmt;

static LOGGER: Logger = Logger;

/// Initialize the logger.
///
/// This function should only be called once.
pub(crate) fn init_logger(level: log::LevelFilter) {
    match log::set_logger(&LOGGER) {
        Ok(()) => (),
        Err(e) => {
            log::error!("Error initializing logger: {e}");
            return;
        }
    }
    log::set_max_level(level);
}

/// The logger to use.
struct Logger;

impl log::Log for Logger {
    fn log(&self, record: &log::Record) {
        use core::fmt::Write as _;

        _ = writeln!(
            crate::sbi::SbiPutcharWriter,
            // TODO I'd like to color these logs
            "{level:>8 } - {source} - {args}",
            level = record.level(),
            source = SourceLogWriter {
                file: record.file(),
                line: record.line()
            },
            args = record.args(),
        );
    }

    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn flush(&self) {
        // We write everything out immediately.
    }
}

struct SourceLogWriter<'a> {
    file: Option<&'a str>,
    line: Option<u32>,
}
impl fmt::Display for SourceLogWriter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self {
                file,
                line: Some(line),
            } => write!(f, "{}:{line}", file.unwrap_or("<unknown>")),
            Self {
                file: Some(file),
                line: None,
            } => f.write_str(file),
            Self {
                file: None,
                line: None,
            } => f.write_str("<unknown loc>"),
        }
    }
}
