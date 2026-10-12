use command_group::AsyncGroupChild;
#[cfg(unix)]
use tokio::time::Duration;

/// Stops a process group: SIGINT, then SIGTERM, then SIGKILL, two seconds
/// apart, stopping early once the group is gone.
pub async fn kill_process_group(child: &mut AsyncGroupChild) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        // Use command_group's UnixChildExt::signal() which calls killpg()
        // with the pgid captured at spawn time. This works even after the
        // group leader has exited, unlike getpgid() which would fail.
        use command_group::{Signal, UnixChildExt};

        for sig in [Signal::SIGINT, Signal::SIGTERM, Signal::SIGKILL] {
            tracing::debug!("Sending {:?} to process group", sig);
            if let Err(e) = child.signal(sig) {
                if group_is_gone(&e) {
                    tracing::debug!("Process group already gone ({e}); nothing to stop");
                    break;
                }
                tracing::warn!("Failed to send signal {:?} to process group: {}", sig, e);
            }
            if sig != Signal::SIGKILL {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }

    let _ = child.kill().await;
    let _ = child.wait().await;
    Ok(())
}

/// Whether a `killpg` failure means there is nothing left to signal. ESRCH is
/// the portable answer; macOS answers EPERM instead when the only member left
/// is the exited, not yet reaped group leader.
#[cfg(unix)]
fn group_is_gone(error: &std::io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(nix::libc::ESRCH) | Some(nix::libc::EPERM)
    )
}

#[cfg(all(test, unix))]
mod tests {
    use super::group_is_gone;

    #[test]
    fn an_empty_group_is_recognised_whichever_errno_the_os_picks() {
        for errno in [nix::libc::ESRCH, nix::libc::EPERM] {
            assert!(group_is_gone(&std::io::Error::from_raw_os_error(errno)));
        }
        assert!(!group_is_gone(&std::io::Error::from_raw_os_error(
            nix::libc::EINVAL
        )));
    }
}
