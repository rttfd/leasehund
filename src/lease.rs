//! Leases as stored by the server and as reported to callers.

use core::net::Ipv4Addr;

/// Longest client host name kept with a lease, in bytes.
pub const MAX_HOSTNAME_LEN: usize = 32;

/// A DHCP lease entry for a client.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct LeaseEntry {
    pub ip: Ipv4Addr,
    pub mac: [u8; 6],
    pub expires_at: u64,
    /// `false` while the address is only reserved for an outstanding offer.
    pub bound: bool,
    pub hostname: Option<Hostname>,
}

impl LeaseEntry {
    pub const fn to_lease(self) -> Lease {
        Lease {
            ip: self.ip,
            mac: self.mac,
            expires_at_ms: self.expires_at,
            bound: self.bound,
            hostname: self.hostname,
        }
    }
}

/// A client's host name (DHCP option 12), as sent by the client.
///
/// Names longer than [`MAX_HOSTNAME_LEN`] bytes are cut to that length (at a
/// character boundary). It is whatever the client claims, so do not trust it
/// for anything but display and logging.
#[derive(PartialEq, Eq, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Hostname {
    bytes: [u8; MAX_HOSTNAME_LEN],
    len: u8,
}

impl Hostname {
    /// Parse the value of option 12: trailing NUL bytes are dropped, invalid
    /// UTF-8 ends the name, and an empty result is `None`.
    #[must_use]
    pub fn parse(value: &[u8]) -> Option<Self> {
        let value = &value[..value.len().min(MAX_HOSTNAME_LEN)];
        let valid = match core::str::from_utf8(value) {
            Ok(text) => text,
            // The prefix up to the error is valid UTF-8.
            Err(error) => core::str::from_utf8(&value[..error.valid_up_to()]).ok()?,
        };
        let name = valid.trim_end_matches('\0');
        if name.is_empty() {
            return None;
        }
        let mut bytes = [0; MAX_HOSTNAME_LEN];
        bytes[..name.len()].copy_from_slice(name.as_bytes());
        Some(Self {
            bytes,
            len: u8::try_from(name.len()).ok()?,
        })
    }

    /// The host name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        // Only valid UTF-8 is ever stored.
        core::str::from_utf8(&self.bytes[..usize::from(self.len)]).unwrap_or_default()
    }
}

impl core::fmt::Debug for Hostname {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(self.as_str(), f)
    }
}

impl core::fmt::Display for Hostname {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A lease as reported by [`DhcpServer::leases`](leasehund::DhcpServer::leases).
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Lease {
    /// The leased address.
    pub ip: Ipv4Addr,
    /// The client's MAC address.
    pub mac: [u8; 6],
    /// When the lease (or offer reservation) expires, in milliseconds of
    /// `embassy_time::Instant`. `u64::MAX` for an infinite lease.
    pub expires_at_ms: u64,
    /// `true` once the client accepted the lease (`DHCPACK` sent), `false`
    /// while the address is only reserved for an offer.
    pub bound: bool,
    /// The host name the client sent (option 12), if any.
    pub hostname: Option<Hostname>,
}

/// Why [`DhcpServer::add_static_lease`](leasehund::DhcpServer::add_static_lease) failed.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum StaticLeaseError {
    /// All `MAX_STATIC` static lease slots are in use.
    Full,
    /// The address is already reserved for a different MAC address.
    AddressTaken,
    /// The address is not inside the server's subnet or any [`SubnetPool`](leasehund::SubnetPool),
    /// so it could never be handed out.
    OutsideSubnet,
    /// The address is the server's own, the router's, or the network or
    /// broadcast address of the subnet.
    ReservedAddress,
}
