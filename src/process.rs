//! Child-process ownership and interruptible descriptor waits for Unix tools.
use std::{io, os::fd::AsRawFd, process::Child, time::Duration};

pub struct Process(pub Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Wait for data or disconnection. An interrupted wait returns to the caller,
/// allowing signal flags and cancellation to be checked before waiting again.
pub fn wait_readable(fd: &impl AsRawFd, timeout: Duration) -> io::Result<bool> {
    let mut poll = libc::pollfd {
        fd: fd.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    let millis = timeout.as_millis().min(i32::MAX as u128) as i32;
    // SAFETY: the borrowed descriptor and the pollfd remain valid for this call.
    let result = unsafe { libc::poll(&mut poll, 1, millis) };
    if result < 0 {
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::Interrupted {
            return Ok(false);
        }
        return Err(error);
    }
    if poll.revents & libc::POLLNVAL != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid descriptor",
        ));
    }
    Ok(result > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, os::unix::net::UnixStream};

    #[test]
    fn descriptor_wait_observes_timeout_data_and_disconnect() {
        let (read, mut write) = UnixStream::pair().unwrap();
        assert!(!wait_readable(&read, Duration::ZERO).unwrap());
        write.write_all(b"ready").unwrap();
        assert!(wait_readable(&read, Duration::from_secs(1)).unwrap());
        drop(write);
        assert!(wait_readable(&read, Duration::ZERO).unwrap());
    }
}
