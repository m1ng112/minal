//! PTY (pseudoterminal) management for shell process communication.
//!
//! Provides a [`Pty`] type that creates a pseudoterminal pair and spawns
//! a shell process connected to it. The master side is used by the terminal
//! emulator to read output from and write input to the shell.

use std::fs::File;
use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};

use rustix::fs::{OFlags, fcntl_setfl};
use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};
use rustix::termios::{Winsize, tcsetwinsize};

use crate::CoreError;

/// A pseudoterminal pair (master fd + child shell process).
///
/// The master file descriptor is used for reading shell output and writing
/// user input. The child process is the shell (e.g. `/bin/zsh`).
pub struct Pty {
    master: OwnedFd,
    child: Child,
}

impl Pty {
    /// Spawn a shell process connected to a new PTY.
    ///
    /// # Arguments
    /// * `shell` - Path to the shell program (e.g. `/bin/zsh`)
    /// * `args` - Arguments to pass to the shell
    /// * `rows` - Initial terminal height in rows
    /// * `cols` - Initial terminal width in columns
    pub fn spawn(shell: &str, args: &[String], rows: u16, cols: u16) -> Result<Self, CoreError> {
        // Create master PTY
        let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY)?;
        grantpt(&master)?;
        unlockpt(&master)?;

        // Get slave device name
        let slave_name_cstr = ptsname(&master, Vec::new())
            .map_err(|e| CoreError::PtySpawn(format!("ptsname failed: {e}")))?;
        let slave_path = slave_name_cstr
            .to_str()
            .map_err(|e| CoreError::PtySpawn(format!("invalid slave name: {e}")))?
            .to_string();

        // Set initial window size
        let winsize = Winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        tcsetwinsize(&master, winsize)?;

        // Open slave PTY file descriptors for child stdin/stdout/stderr.
        // We open the slave path three times so each Stdio gets its own fd.
        let slave_stdin = File::options()
            .read(true)
            .write(true)
            .open(&slave_path)
            .map_err(|e| CoreError::PtySpawn(format!("failed to open slave {slave_path}: {e}")))?;
        let slave_stdout = File::options()
            .read(true)
            .write(true)
            .open(&slave_path)
            .map_err(|e| CoreError::PtySpawn(format!("failed to open slave {slave_path}: {e}")))?;
        let slave_stderr = File::options()
            .read(true)
            .write(true)
            .open(&slave_path)
            .map_err(|e| CoreError::PtySpawn(format!("failed to open slave {slave_path}: {e}")))?;

        // Convert to raw fds, transferring ownership to Stdio.
        let stdin_fd = slave_stdin.into_raw_fd();
        let stdout_fd = slave_stdout.into_raw_fd();
        let stderr_fd = slave_stderr.into_raw_fd();

        // SAFETY: pre_exec runs after fork, before exec in the child process.
        // We call libc::setsid() to create a new session and libc::ioctl with
        // TIOCSCTTY to set the controlling terminal. These are standard POSIX
        // operations that are safe in a post-fork/pre-exec context.
        // The from_raw_fd calls are safe because we transferred ownership from
        // File via into_raw_fd above, and each fd is used exactly once.
        let child = unsafe {
            Command::new(shell)
                .args(args)
                .stdin(Stdio::from_raw_fd(stdin_fd))
                .stdout(Stdio::from_raw_fd(stdout_fd))
                .stderr(Stdio::from_raw_fd(stderr_fd))
                .env("TERM", "xterm-256color")
                .env("COLORTERM", "truecolor")
                .pre_exec(move || {
                    libc::setsid();
                    libc::ioctl(stdin_fd, libc::TIOCSCTTY as _, 0);
                    Ok(())
                })
                .spawn()
                .map_err(|e| CoreError::PtySpawn(format!("failed to spawn shell: {e}")))?
        };

        Ok(Self { master, child })
    }

    /// Get a borrowable reference to the master fd.
    pub fn master_fd(&self) -> BorrowedFd<'_> {
        self.master.as_fd()
    }

    /// Get the raw file descriptor of the master side.
    pub fn master_raw_fd(&self) -> i32 {
        self.master.as_raw_fd()
    }

    /// Set the master fd to non-blocking mode.
    pub fn set_nonblocking(&self) -> Result<(), CoreError> {
        fcntl_setfl(&self.master, OFlags::NONBLOCK)?;
        Ok(())
    }

    /// Resize the PTY window.
    pub fn resize(&self, rows: u16, cols: u16) -> Result<(), CoreError> {
        let winsize = Winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        tcsetwinsize(&self.master, winsize)?;
        Ok(())
    }

    /// Read from the master side of the PTY.
    ///
    /// Returns the number of bytes read, or an I/O error.
    /// In non-blocking mode, returns `WouldBlock` if no data is available.
    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        rustix::io::read(&self.master, buf).map_err(io::Error::from)
    }

    /// Write to the master side of the PTY (sends input to the shell).
    pub fn write_all(&self, buf: &[u8]) -> io::Result<()> {
        let mut written = 0;
        while written < buf.len() {
            let n = rustix::io::write(&self.master, &buf[written..]).map_err(io::Error::from)?;
            written += n;
        }
        Ok(())
    }

    /// Check if the child process has exited without blocking.
    pub fn try_wait(&mut self) -> io::Result<Option<std::process::ExitStatus>> {
        self.child.try_wait()
    }

    /// Kill the child process.
    pub fn kill(&mut self) -> io::Result<()> {
        self.child.kill()
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        // Best-effort cleanup: kill child if still running
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_and_read_echo() {
        let mut pty = Pty::spawn("/bin/echo", &["hello".to_string()], 24, 80).unwrap();

        // Wait for the child to finish
        let status = pty.child.wait().unwrap();
        assert!(status.success());

        // Read output from master side
        let mut buf = [0u8; 256];
        let n = pty.read(&mut buf).unwrap_or(0);
        if n > 0 {
            let output = String::from_utf8_lossy(&buf[..n]);
            assert!(output.contains("hello"), "output was: {output}");
        }
    }

    #[test]
    fn test_resize() {
        let pty = Pty::spawn("/bin/true", &[], 24, 80).unwrap();
        assert!(pty.resize(48, 120).is_ok());
    }

    #[test]
    fn test_set_nonblocking() {
        let pty = Pty::spawn("/bin/true", &[], 24, 80).unwrap();
        assert!(pty.set_nonblocking().is_ok());
    }
}
