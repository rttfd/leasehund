//! The DHCP wire format: message types, the fixed packet header, and the
//! fields of a client message the server acts on.

use core::net::Ipv4Addr;
use leasehund::lease::Hostname;
use leasehund::options;

/// Standard DHCP server port (RFC 2131)
pub const DHCP_SERVER_PORT: u16 = 67;
/// Standard DHCP client port (RFC 2131)
pub const DHCP_CLIENT_PORT: u16 = 68;

// DHCP Message Types (RFC 2131)
pub const DHCP_DISCOVER: u8 = 1;
pub const DHCP_OFFER: u8 = 2;
pub const DHCP_REQUEST: u8 = 3;
pub const DHCP_DECLINE: u8 = 4;
pub const DHCP_ACK: u8 = 5;
pub const DHCP_NAK: u8 = 6;
pub const DHCP_RELEASE: u8 = 7;
pub const DHCP_INFORM: u8 = 8;

/// BOOTP operation codes (RFC 951).
pub const BOOTREQUEST: u8 = 1;
pub const BOOTREPLY: u8 = 2;
/// Hardware type and address length of Ethernet.
pub const HTYPE_ETHERNET: u8 = 1;
pub const HLEN_ETHERNET: u8 = 6;
/// The broadcast bit of the `flags` field.
pub const BROADCAST_FLAG: u16 = 0x8000;

// The standard DHCP magic cookie (0x63825363).
// Required by RFC 2132 section 2.
pub const DHCP_MAGIC: [u8; 4] = [0x63, 0x82, 0x53, 0x63];

/// DHCP packet structure as defined in [RFC 2131](https://www.rfc-editor.org/rfc/rfc2131).
///
/// Packed to match the wire format exactly. Multi-byte integers hold the raw
/// network-order bytes; convert with `from_be`/`to_be`.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct DhcpPacket {
    pub op: u8,
    pub htype: u8,
    pub hlen: u8,
    pub hops: u8,
    pub xid: u32,
    pub secs: u16,
    pub flags: u16,
    pub ciaddr: [u8; 4],
    pub yiaddr: [u8; 4],
    pub siaddr: [u8; 4],
    pub giaddr: [u8; 4],
    pub chaddr: [u8; 16],
    pub sname: [u8; 64],
    pub file: [u8; 128],
    pub magic: [u8; 4],
}

impl Default for DhcpPacket {
    fn default() -> Self {
        Self {
            op: 0,
            htype: 0,
            hlen: 0,
            hops: 0,
            xid: 0,
            secs: 0,
            flags: 0,
            ciaddr: [0; 4],
            yiaddr: [0; 4],
            siaddr: [0; 4],
            giaddr: [0; 4],
            chaddr: [0; 16],
            sname: [0; 64],
            file: [0; 128],
            magic: DHCP_MAGIC,
        }
    }
}

pub const FIXED_PART_SIZE: usize = core::mem::size_of::<DhcpPacket>();
pub const OPTIONS_SIZE: usize = 335;
pub const DHCP_PACKET_SIZE: usize = FIXED_PART_SIZE + OPTIONS_SIZE + 1; // +1 for END option

/// The fields of a client message the server acts on.
pub struct ClientMessage {
    pub packet: DhcpPacket,
    pub msg_type: u8,
    pub mac: [u8; 6],
    pub ciaddr: Option<Ipv4Addr>,
    pub giaddr: Option<Ipv4Addr>,
    pub requested_ip: Option<Ipv4Addr>,
    pub server_id: Option<Ipv4Addr>,
    pub hostname: Option<Hostname>,
}

/// `None` for the unspecified address `0.0.0.0`.
pub fn non_zero(octets: [u8; 4]) -> Option<Ipv4Addr> {
    let ip = Ipv4Addr::from(octets);
    (!ip.is_unspecified()).then_some(ip)
}

impl ClientMessage {
    /// Parse and sanity-check a datagram received on the server port.
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < FIXED_PART_SIZE {
            return None;
        }
        // SAFETY: read_unaligned handles the packed struct without alignment requirements.
        // We verified data.len() >= FIXED_PART_SIZE above.
        let packet = unsafe { core::ptr::read_unaligned(data.as_ptr().cast::<DhcpPacket>()) };
        if packet.magic != DHCP_MAGIC
            || packet.op != BOOTREQUEST
            || packet.htype != HTYPE_ETHERNET
            || packet.hlen != HLEN_ETHERNET
        {
            return None;
        }

        let options = &data[FIXED_PART_SIZE..];
        let mut mac = [0; 6];
        mac.copy_from_slice(&packet.chaddr[..6]);
        Some(Self {
            packet,
            msg_type: options::message_type(options)?,
            mac,
            ciaddr: non_zero(packet.ciaddr),
            giaddr: non_zero(packet.giaddr),
            requested_ip: options::find_ipv4(options, options::REQUESTED_IP),
            server_id: options::find_ipv4(options, options::SERVER_ID),
            hostname: options::find(options, options::HOST_NAME).and_then(Hostname::parse),
        })
    }
}
