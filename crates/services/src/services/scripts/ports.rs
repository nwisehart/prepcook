//! Ports for app runs.
//!
//! Each run gets one port from a configured range (`app_runs.port_range`,
//! `4200-4299` by default). The registry is in memory and keyed by run id, so a
//! port is never handed to two runs this process started; the bind check before
//! handing one out covers everything else on the machine that might already be
//! listening there.

use std::{
    collections::HashMap,
    net::{Ipv4Addr, TcpListener},
    ops::RangeInclusive,
    sync::Mutex,
};

use uuid::Uuid;

pub const DEFAULT_PORT_RANGE: &str = "4200-4299";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PortError {
    #[error("invalid port range {0:?}: expected <first>-<last>, e.g. 4200-4299")]
    InvalidRange(String),
    #[error("no free port in {first}-{last}: every port is taken by a run or another process")]
    Exhausted { first: u16, last: u16 },
}

/// Parses `"4200-4299"` (inclusive). A single port (`"4200"`) is a range of
/// one. Port 0 and a reversed range are refused.
pub fn parse_range(raw: &str) -> Result<RangeInclusive<u16>, PortError> {
    let invalid = || PortError::InvalidRange(raw.to_string());
    let raw_trimmed = raw.trim();
    let (first, last) = match raw_trimmed.split_once('-') {
        Some((first, last)) => (first.trim(), last.trim()),
        None => (raw_trimmed, raw_trimmed),
    };
    let first: u16 = first.parse().map_err(|_| invalid())?;
    let last: u16 = last.parse().map_err(|_| invalid())?;
    if first == 0 || last < first {
        return Err(invalid());
    }
    Ok(first..=last)
}

/// Whether nothing is listening on `port` on the loopback interface.
fn port_is_free(port: u16) -> bool {
    TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok()
}

/// Ports handed out, by run.
#[derive(Debug, Default)]
pub struct PortAllocator {
    in_use: Mutex<HashMap<u16, Uuid>>,
}

impl PortAllocator {
    pub fn new() -> Self {
        Self::default()
    }

    /// The first port in `range` that no run holds and nothing is bound to,
    /// recorded against `run_id`. A run that already holds a port gets it back.
    pub fn allocate(&self, range: &RangeInclusive<u16>, run_id: Uuid) -> Result<u16, PortError> {
        self.allocate_with(range, run_id, port_is_free)
    }

    /// [`Self::allocate`], trying `preferred` first when it is free. Lets a
    /// restarted app come back on the port its agent session was told about.
    pub fn allocate_preferring(
        &self,
        range: &RangeInclusive<u16>,
        run_id: Uuid,
        preferred: Option<u16>,
    ) -> Result<u16, PortError> {
        self.allocate_inner(range, run_id, preferred, port_is_free)
    }

    fn allocate_with(
        &self,
        range: &RangeInclusive<u16>,
        run_id: Uuid,
        is_free: impl Fn(u16) -> bool,
    ) -> Result<u16, PortError> {
        self.allocate_inner(range, run_id, None, is_free)
    }

    fn allocate_inner(
        &self,
        range: &RangeInclusive<u16>,
        run_id: Uuid,
        preferred: Option<u16>,
        is_free: impl Fn(u16) -> bool,
    ) -> Result<u16, PortError> {
        let mut in_use = self.in_use.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((port, _)) = in_use.iter().find(|(_, holder)| **holder == run_id) {
            return Ok(*port);
        }
        let port = preferred
            .into_iter()
            .chain(range.clone())
            .find(|port| *port != 0 && !in_use.contains_key(port) && is_free(*port))
            .ok_or(PortError::Exhausted {
                first: *range.start(),
                last: *range.end(),
            })?;
        in_use.insert(port, run_id);
        Ok(port)
    }

    /// Gives back whatever `run_id` holds. Safe for a run that holds nothing.
    pub fn release(&self, run_id: Uuid) {
        self.in_use
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .retain(|_, holder| *holder != run_id);
    }

    pub fn port_of(&self, run_id: Uuid) -> Option<u16> {
        self.in_use
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .find(|(_, holder)| **holder == run_id)
            .map(|(port, _)| *port)
    }

    pub fn in_use(&self) -> usize {
        self.in_use.lock().unwrap_or_else(|p| p.into_inner()).len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_parse_and_bad_ones_are_refused() {
        assert_eq!(parse_range("4200-4299"), Ok(4200..=4299));
        assert_eq!(parse_range(" 5000 - 5001 "), Ok(5000..=5001));
        assert_eq!(parse_range("4300"), Ok(4300..=4300));
        for bad in ["", "abc", "4299-4200", "0-10", "4200-70000", "-4200"] {
            assert!(
                matches!(parse_range(bad), Err(PortError::InvalidRange(_))),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn ports_run_out_and_come_back_on_release() {
        let ports = PortAllocator::new();
        let range = 6000..=6001;
        let (a, b, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        assert_eq!(ports.allocate_with(&range, a, |_| true), Ok(6000));
        // Asking again returns the same port rather than a second one.
        assert_eq!(ports.allocate_with(&range, a, |_| true), Ok(6000));
        assert_eq!(ports.allocate_with(&range, b, |_| true), Ok(6001));
        assert_eq!(
            ports.allocate_with(&range, c, |_| true),
            Err(PortError::Exhausted {
                first: 6000,
                last: 6001
            })
        );
        ports.release(a);
        assert_eq!(ports.port_of(a), None);
        assert_eq!(ports.allocate_with(&range, c, |_| true), Ok(6000));
        assert_eq!(ports.in_use(), 2);
        ports.release(Uuid::new_v4()); // holds nothing: no-op
        assert_eq!(ports.in_use(), 2);
    }

    #[test]
    fn a_port_something_else_is_bound_to_is_skipped() {
        let ports = PortAllocator::new();
        let range = 6100..=6102;
        let port = ports
            .allocate_with(&range, Uuid::new_v4(), |port| port != 6100)
            .unwrap();
        assert_eq!(port, 6101);

        // And the real bind check sees a real listener.
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let taken = listener.local_addr().unwrap().port();
        assert!(!port_is_free(taken));
        let only = taken..=taken;
        assert!(matches!(
            ports.allocate(&only, Uuid::new_v4()),
            Err(PortError::Exhausted { .. })
        ));
    }
}
