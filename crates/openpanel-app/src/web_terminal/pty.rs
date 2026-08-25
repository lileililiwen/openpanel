//! PTY adapter backed by `portable-pty`. Spawns the site user's shell
//! in a pseudo-terminal; all privilege handling stays inside the
//! crate's safe API.

use std::io::Write;

use openpanel_domain::web_terminal::{PtyPort, PtySpec, PtyStream, TerminalError};

/// Production [`PtyPort`] using the host's pty system.
pub struct PortablePtyAdapter {
    /// Shell to spawn when the spec does not name one.
    pub shell: String,
}

impl PortablePtyAdapter {
    /// Construct the adapter with a shell override.
    pub fn new(shell: impl Into<String>) -> Self {
        Self {
            shell: shell.into(),
        }
    }
}

impl PtyPort for PortablePtyAdapter {
    fn open(&self, spec: &PtySpec) -> Result<Box<dyn PtyStream>, TerminalError> {
        use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};

        let system = NativePtySystem::default();
        let pair = system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| TerminalError::Failure(bounded(&error.to_string())))?;

        // When a site user is named, drop privileges through `su`
        // (safe, standard tooling); otherwise run as the current user.
        let shell = if spec.shell.is_empty() {
            self.shell.clone()
        } else {
            spec.shell.clone()
        };
        let command = if spec.user.is_empty() {
            let mut cmd = CommandBuilder::new(&shell);
            if !spec.cwd.is_empty() {
                cmd.cwd(&spec.cwd);
            }
            cmd
        } else {
            let inner = if spec.cwd.is_empty() {
                format!("exec {shell}")
            } else {
                format!("cd '{}' && exec {shell}", spec.cwd)
            };
            let mut cmd = CommandBuilder::new("su");
            cmd.arg("-s");
            cmd.arg(&shell);
            cmd.arg("-c");
            cmd.arg(inner);
            cmd.arg("--");
            cmd.arg(&spec.user);
            cmd
        };

        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| TerminalError::Failure(bounded(&error.to_string())))?;
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| TerminalError::Failure(bounded(&error.to_string())))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| TerminalError::Failure(bounded(&error.to_string())))?;

        Ok(Box::new(PortablePtyStream {
            reader,
            writer,
            child,
        }))
    }
}

struct PortablePtyStream {
    reader: Box<dyn std::io::Read + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
}

impl PtyStream for PortablePtyStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.reader.read(buf)
    }

    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.writer.write_all(buf)?;
        self.writer.flush()?;
        Ok(buf.len())
    }

    fn has_exited(&mut self) -> std::io::Result<bool> {
        Ok(self
            .child
            .try_wait()
            .map_err(|error| std::io::Error::other(error.to_string()))?
            .is_some())
    }
}

fn bounded(value: &str) -> String {
    value.chars().take(512).collect()
}
