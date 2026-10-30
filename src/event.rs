//! Events reported to callers and to admission policies.

use core::net::Ipv4Addr;

/// Represents a DHCP lease or release event.
///
/// # Examples
///
/// ```rust
/// use leasehund::TransactionEvent;
/// use core::net::Ipv4Addr;
/// let event = TransactionEvent::Leased(Ipv4Addr::new(192, 168, 1, 100), [0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);
/// ```
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum TransactionEvent {
    /// A new lease was assigned.
    Leased(Ipv4Addr, [u8; 6]),
    /// A client released its IP.
    Released(Ipv4Addr, [u8; 6]),
}

/// DHCP message kinds reported to an admission policy.
///
/// Returning `false` for [`Discover`](Self::Discover), [`Request`](Self::Request) or
/// [`Inform`](Self::Inform) silently suppresses the corresponding response. A client's MAC
/// address is not an authentication mechanism and can be spoofed, so admission policies should
/// not be treated as a security boundary.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum AdmissionEvent {
    /// Client is asking for an offer.
    Discover,
    /// Client is asking for a lease acknowledgment.
    Request,
    /// Client is releasing an existing lease.
    ///
    /// Releases are always processed; the policy's return value is ignored. Completed releases
    /// are also reported through [`TransactionEvent::Released`].
    Release,
    /// Client already has an address and asks only for configuration (`DHCPINFORM`).
    Inform,
}
