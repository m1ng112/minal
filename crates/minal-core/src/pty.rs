//! PTY (pseudoterminal) management.
//!
//! Provides [`Pty`] for synchronous PTY operations (open, resize, wait) and
//! [`AsyncPty`] for tokio-based non-blocking I/O on the master file descriptor.

use std::ffi::CString;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd, RawFd};

use rustix::process::{Pid, WaitOptions};
use rustix::pty::{OpenptFlags, openpt};
use rustix::termios::{self, Winsize};
use tokio::io::unix::AsyncFd;

use crate::CoreError;

/// Terminal dimensions for PTY sizing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtySize {
    /// Number of character rows.
    pub rows: u16,
    /// Number of character columns.
    pub cols: u16,
    /// Pixel width of the terminal window.
    pub pixel_width: u16,
    /// Pixel height of the terminal window.
    pub pixel_height: u16,
}

impl PtySize {
    /// Create a new `PtySize` with the given character dimensions and zero pixel dimensions.
    pub fn new(rows: u16, cols: u16) -> Self {
        Self {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        }
    }

    /// Convert to a rustix [`Winsize`].
    fn to_winsize(self) -> Winsize {
        Winsize {
            ws_row: self.rows,
            ws_col: self.cols,
            ws_xpixel: self.pixel_width,
            ws_ypixel: self.pixel_height,
        }
    }
}

/// A pseudoterminal with a master file descriptor and child process.
///
/// The master fd is used for reading/writing data to/from the child process.
/// When dropped, the master fd is closed automatically via [`OwnedFd`].
pub struct Pty {
    master: OwnedFd,
    child_pid: u32,
}

impl std::fmt::Debug for Pty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pty")
            .field("master_fd", &self.master.as_raw_fd())
            .field("child_pid", &self.child_pid)
            .finish()
    }
}

impl Pty {
    /// Open a PTY, fork, and spawn the given shell in the child process.
    ///
    /// The child process will have the slave side of the PTY as its
    /// stdin/stdout/stderr and the specified environment variables set.
    ///
    /// # Arguments
    ///
    /// * `shell` - Path to the shell to execute (e.g., `/bin/bash`).
    /// * `size` - Initial terminal dimensions.
    /// * `env_vars` - Additional environment variables to set in the child.
    pub fn open(
        shell: &str,
        size: PtySize,
        env_vars: &[(String, String)],
    ) -> Result<Self, CoreError> {
        // Open the master side of the PTY.
        let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY)
            .map_err(|e| CoreError::PtySetup(format!("openpt failed: {e}")))?;

        // Grant and unlock the slave side.
        rustix::pty::grantpt(&master)
            .map_err(|e| CoreError::PtySetup(format!("grantpt failed: {e}")))?;
        rustix::pty::unlockpt(&master)
            .map_err(|e| CoreError::PtySetup(format!("unlockpt failed: {e}")))?;

        // Get the slave device name.
        let slave_name = rustix::pty::ptsname(&master, Vec::new())
            .map_err(|e| CoreError::PtySetup(format!("ptsname failed: {e}")))?;

        tracing::debug!(
            master_fd = master.as_raw_fd(),
            slave = ?slave_name,
            "PTY master opened"
        );

        // Set the initial window size on the master.
        termios::tcsetwinsize(&master, size.to_winsize())
            .map_err(|e| CoreError::PtySetup(format!("tcsetwinsize failed: {e}")))?;

        // Build environment for the child process.
        let mut child_env: Vec<CString> = Vec::new();

        // Pass through important environment variables from the parent.
        for key in &["HOME", "USER", "PATH", "SHELL", "LANG", "LC_ALL", "LOGNAME"] {
            if let Ok(val) = std::env::var(key) {
                let entry = format!("{key}={val}");
                if let Ok(cs) = CString::new(entry) {
                    child_env.push(cs);
                }
            }
        }

        // Set TERM explicitly.
        if let Ok(cs) = CString::new("TERM=xterm-256color") {
            child_env.push(cs);
        }

        // Add caller-provided env vars (may override the above).
        for (k, v) in env_vars {
            let entry = format!("{k}={v}");
            if let Ok(cs) = CString::new(entry) {
                child_env.push(cs);
            }
        }

        let env_ptrs: Vec<*const libc::c_char> = child_env
            .iter()
            .map(|cs| cs.as_ptr())
            .chain(std::iter::once(std::ptr::null()))
            .collect();

        // Prepare shell argv.
        let shell_cstr = CString::new(shell)
            .map_err(|e| CoreError::PtySetup(format!("invalid shell path: {e}")))?;
        let argv: [*const libc::c_char; 2] = [shell_cstr.as_ptr(), std::ptr::null()];

        // Fork the process.
        // SAFETY: fork() is an inherently unsafe operation. We immediately call
        // only async-signal-safe functions in the child (setsid, open, dup2,
        // close, ioctl, execve) and do not allocate or touch shared state.
        // The parent continues normally after fork returns.
        let pid = unsafe { libc::fork() };

        match pid {
            -1 => {
                let err = std::io::Error::last_os_error();
                Err(CoreError::ForkFailed(format!("fork failed: {err}")))
            }
            0 => {
                // === Child process ===
                // All calls here must be async-signal-safe.

                // SAFETY: setsid is async-signal-safe. We call it to create a
                // new session so the child is the session leader.
                unsafe {
                    if libc::setsid() == -1 {
                        libc::_exit(1);
                    }
                }

                // Open the slave PTY.
                // SAFETY: We open the slave device by its path name. This is
                // async-signal-safe (open is listed in POSIX async-signal-safe).
                let slave_fd = unsafe { libc::open(slave_name.as_ptr(), libc::O_RDWR) };
                if slave_fd < 0 {
                    // SAFETY: _exit is async-signal-safe.
                    unsafe {
                        libc::_exit(1);
                    }
                }

                // Set the slave as the controlling terminal.
                // SAFETY: ioctl(TIOCSCTTY) is async-signal-safe.
                unsafe {
                    if libc::ioctl(slave_fd, libc::TIOCSCTTY as libc::c_ulong, 0i32) == -1 {
                        libc::_exit(1);
                    }
                }

                // Duplicate slave fd to stdin/stdout/stderr.
                // SAFETY: dup2 is async-signal-safe.
                unsafe {
                    if libc::dup2(slave_fd, libc::STDIN_FILENO) == -1 {
                        libc::_exit(1);
                    }
                    if libc::dup2(slave_fd, libc::STDOUT_FILENO) == -1 {
                        libc::_exit(1);
                    }
                    if libc::dup2(slave_fd, libc::STDERR_FILENO) == -1 {
                        libc::_exit(1);
                    }
                }

                // Close the original slave fd if it's not one of 0/1/2.
                if slave_fd > 2 {
                    // SAFETY: close is async-signal-safe.
                    unsafe {
                        libc::close(slave_fd);
                    }
                }

                // Close the master fd in the child (inherited from parent).
                // SAFETY: close is async-signal-safe. We use the raw fd to avoid
                // double-close when OwnedFd drops (which won't happen because
                // we exec or _exit below).
                unsafe {
                    libc::close(master.as_raw_fd());
                }

                // Execute the shell.
                // SAFETY: execve is async-signal-safe. argv and envp are valid
                // null-terminated arrays pointing to valid C strings.
                unsafe {
                    libc::execve(shell_cstr.as_ptr(), argv.as_ptr(), env_ptrs.as_ptr());
                    // If execve returns, it failed.
                    libc::_exit(127);
                }
            }
            child_pid => {
                // === Parent process ===
                tracing::info!(child_pid, shell, "spawned child process");
                Ok(Self {
                    master,
                    child_pid: child_pid as u32,
                })
            }
        }
    }

    /// Resize the PTY to the given dimensions.
    pub fn resize(&self, size: PtySize) -> Result<(), CoreError> {
        termios::tcsetwinsize(&self.master, size.to_winsize())
            .map_err(|e| CoreError::PtySetup(format!("resize failed: {e}")))?;
        tracing::debug!(rows = size.rows, cols = size.cols, "PTY resized");
        Ok(())
    }

    /// Return a borrowed reference to the master file descriptor.
    pub fn master_fd(&self) -> BorrowedFd<'_> {
        self.master.as_fd()
    }

    /// Return the child process ID.
    pub fn child_pid(&self) -> u32 {
        self.child_pid
    }

    /// Check if the child process has exited (non-blocking).
    ///
    /// Returns `Ok(Some(exit_code))` if the child has exited,
    /// `Ok(None)` if it is still running, or an error on failure.
    pub fn try_wait(&self) -> Result<Option<i32>, CoreError> {
        let pid = Pid::from_raw(self.child_pid as i32)
            .ok_or_else(|| CoreError::PtySetup("invalid child PID".to_string()))?;

        match rustix::process::waitpid(Some(pid), WaitOptions::NOHANG) {
            Ok(Some((_pid, status))) => {
                if let Some(code) = status.exit_status() {
                    Ok(Some(code))
                } else if status.signaled() {
                    // Killed by signal — return signal number as negative exit code.
                    let sig = status.terminating_signal().unwrap_or(0);
                    Ok(Some(-sig))
                } else {
                    Ok(None)
                }
            }
            Ok(None) => Ok(None),
            Err(e) => Err(CoreError::PtySetup(format!("waitpid failed: {e}"))),
        }
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        // Send SIGHUP to the child process to notify it that the terminal is closing.
        // SAFETY: kill() with a valid pid and signal is safe. We ignore errors because
        // the child may have already exited.
        unsafe {
            libc::kill(self.child_pid as libc::pid_t, libc::SIGHUP);
        }

        // Reap the child to avoid zombies (non-blocking).
        if let Some(pid) = Pid::from_raw(self.child_pid as i32) {
            let _ = rustix::process::waitpid(Some(pid), WaitOptions::NOHANG);
        }
    }
}

/// Wrapper around a raw file descriptor that implements [`AsRawFd`] and [`AsFd`].
///
/// This is needed for [`AsyncFd`] which requires `AsRawFd` on its inner type.
#[derive(Debug)]
struct RawFdWrapper {
    fd: RawFd,
}

impl AsRawFd for RawFdWrapper {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl AsFd for RawFdWrapper {
    fn as_fd(&self) -> BorrowedFd<'_> {
        // SAFETY: The fd is valid for the lifetime of the AsyncPty which
        // owns the underlying Pty (and thus the OwnedFd). We borrow it
        // for at most 'self which is shorter than the Pty lifetime.
        unsafe { BorrowedFd::borrow_raw(self.fd) }
    }
}

/// Async wrapper around a PTY master fd for use with tokio.
///
/// Provides non-blocking read/write via [`AsyncFd`].
pub struct AsyncPty {
    inner: AsyncFd<RawFdWrapper>,
    /// The underlying synchronous PTY (owns the master fd and child pid).
    pty: Pty,
}

impl std::fmt::Debug for AsyncPty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncPty")
            .field("fd", &self.pty.master.as_raw_fd())
            .field("child_pid", &self.pty.child_pid)
            .finish()
    }
}

impl AsyncPty {
    /// Create an [`AsyncPty`] from a synchronous [`Pty`].
    ///
    /// This sets the master fd to non-blocking mode and registers it with
    /// the tokio reactor.
    pub fn from_pty(pty: Pty) -> Result<Self, CoreError> {
        let raw_fd = pty.master.as_raw_fd();

        // Set the fd to non-blocking mode.
        set_nonblocking(raw_fd)?;

        let wrapper = RawFdWrapper { fd: raw_fd };
        let inner = AsyncFd::new(wrapper)
            .map_err(|e| CoreError::PtySetup(format!("AsyncFd creation failed: {e}")))?;

        tracing::debug!(fd = raw_fd, "created AsyncPty");
        Ok(Self { inner, pty })
    }

    /// Asynchronously read from the PTY master.
    ///
    /// Returns the number of bytes read, or an error. Returns 0 on EOF (child
    /// closed the slave side).
    pub async fn read(&self, buf: &mut [u8]) -> Result<usize, CoreError> {
        loop {
            let mut guard = self.inner.readable().await.map_err(|e| {
                CoreError::Pty(std::io::Error::new(
                    e.kind(),
                    format!("readable wait failed: {e}"),
                ))
            })?;

            match guard.try_io(|inner| {
                let fd = inner.as_raw_fd();
                // SAFETY: We read into a valid buffer with the correct length.
                // The fd is valid because the AsyncPty owns the Pty which owns
                // the OwnedFd.
                let n =
                    unsafe { libc::read(fd, buf.as_mut_ptr().cast::<libc::c_void>(), buf.len()) };
                if n < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(n as usize)
                }
            }) {
                Ok(result) => return result.map_err(CoreError::Pty),
                Err(_would_block) => continue,
            }
        }
    }

    /// Asynchronously write to the PTY master.
    ///
    /// Returns the number of bytes written.
    pub async fn write(&self, data: &[u8]) -> Result<usize, CoreError> {
        loop {
            let mut guard = self.inner.writable().await.map_err(|e| {
                CoreError::Pty(std::io::Error::new(
                    e.kind(),
                    format!("writable wait failed: {e}"),
                ))
            })?;

            match guard.try_io(|inner| {
                let fd = inner.as_raw_fd();
                // SAFETY: We write from a valid buffer with the correct length.
                // The fd is valid because the AsyncPty owns the Pty.
                let n =
                    unsafe { libc::write(fd, data.as_ptr().cast::<libc::c_void>(), data.len()) };
                if n < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(n as usize)
                }
            }) {
                Ok(result) => return result.map_err(CoreError::Pty),
                Err(_would_block) => continue,
            }
        }
    }

    /// Resize the underlying PTY.
    pub fn resize(&self, size: PtySize) -> Result<(), CoreError> {
        self.pty.resize(size)
    }

    /// Return the child process ID.
    pub fn child_pid(&self) -> u32 {
        self.pty.child_pid()
    }

    /// Check if the child process has exited (non-blocking).
    pub fn try_wait(&self) -> Result<Option<i32>, CoreError> {
        self.pty.try_wait()
    }
}

/// Set a file descriptor to non-blocking mode.
fn set_nonblocking(fd: RawFd) -> Result<(), CoreError> {
    // SAFETY: fcntl with F_GETFL/F_SETFL is safe on a valid fd.
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags == -1 {
            return Err(CoreError::PtySetup(format!(
                "fcntl F_GETFL failed: {}",
                std::io::Error::last_os_error()
            )));
        }
        if libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) == -1 {
            return Err(CoreError::PtySetup(format!(
                "fcntl F_SETFL failed: {}",
                std::io::Error::last_os_error()
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pty_size_new() {
        let size = PtySize::new(24, 80);
        assert_eq!(size.rows, 24);
        assert_eq!(size.cols, 80);
        assert_eq!(size.pixel_width, 0);
        assert_eq!(size.pixel_height, 0);
    }

    #[test]
    fn test_pty_size_with_pixels() {
        let size = PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 640,
            pixel_height: 480,
        };
        let ws = size.to_winsize();
        assert_eq!(ws.ws_row, 24);
        assert_eq!(ws.ws_col, 80);
        assert_eq!(ws.ws_xpixel, 640);
        assert_eq!(ws.ws_ypixel, 480);
    }

    #[test]
    fn test_pty_open_and_close() {
        let shell = if std::path::Path::new("/bin/sh").exists() {
            "/bin/sh"
        } else {
            return; // Skip test if no shell available.
        };

        let size = PtySize::new(24, 80);
        let pty = Pty::open(shell, size, &[]).expect("failed to open PTY");

        assert!(pty.child_pid() > 0);
        assert!(pty.master_fd().as_raw_fd() >= 0);

        // Child should still be running.
        let status = pty.try_wait().expect("try_wait failed");
        // It might or might not have exited yet (shell waits for input),
        // so we just check the call succeeds.
        let _ = status;
    }

    #[test]
    fn test_pty_open_with_env_var() {
        // Spawn a shell with an additional environment variable.
        let shell = if std::path::Path::new("/bin/sh").exists() {
            "/bin/sh"
        } else {
            return;
        };

        let size = PtySize::new(24, 80);
        let env = vec![("MY_TEST_VAR".to_string(), "hello".to_string())];
        let pty = Pty::open(shell, size, &env).expect("failed to open PTY");

        assert!(pty.child_pid() > 0);
    }

    #[test]
    fn test_pty_resize() {
        let shell = if std::path::Path::new("/bin/sh").exists() {
            "/bin/sh"
        } else {
            return;
        };

        let size = PtySize::new(24, 80);
        let pty = Pty::open(shell, size, &[]).expect("failed to open PTY");

        let new_size = PtySize::new(40, 120);
        pty.resize(new_size).expect("resize failed");
    }

    #[test]
    fn test_pty_size_equality() {
        let a = PtySize::new(24, 80);
        let b = PtySize::new(24, 80);
        let c = PtySize::new(25, 80);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[tokio::test]
    async fn test_async_pty_write_read() {
        let size = PtySize::new(24, 80);
        let pty = Pty::open("/bin/sh", size, &[]).expect("failed to open PTY");
        let async_pty = AsyncPty::from_pty(pty).expect("failed to create AsyncPty");

        // Write a command that produces output.
        let cmd = b"echo hello\n";
        let written = async_pty.write(cmd).await.expect("write failed");
        assert!(written > 0);

        // Read some output (the shell should echo the command and its output).
        let mut buf = [0u8; 1024];
        let n = async_pty.read(&mut buf).await.expect("read failed");
        assert!(n > 0);
    }

    #[tokio::test]
    async fn test_async_pty_resize() {
        let size = PtySize::new(24, 80);
        let pty = Pty::open("/bin/sh", size, &[]).expect("failed to open PTY");
        let async_pty = AsyncPty::from_pty(pty).expect("failed to create AsyncPty");

        let new_size = PtySize::new(30, 100);
        async_pty.resize(new_size).expect("resize failed");
    }
}
