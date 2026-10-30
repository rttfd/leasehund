//! Building replies: where they go and which options they carry.

use core::net::Ipv4Addr;
use heapless::Vec;
use leasehund::options;
use leasehund::packet::{
    BOOTREPLY, BROADCAST_FLAG, ClientMessage, DHCP_ACK, DHCP_CLIENT_PORT, DHCP_MAGIC, DHCP_NAK,
    DHCP_PACKET_SIZE, DHCP_SERVER_PORT, DhcpPacket, FIXED_PART_SIZE, HLEN_ETHERNET, HTYPE_ETHERNET,
};
use leasehund::pool::Subnet;
use leasehund::server::DhcpServer;

impl<
    const MAX_CLIENTS: usize,
    const MAX_DNS: usize,
    const MAX_STATIC: usize,
    const MAX_POOLS: usize,
> DhcpServer<MAX_CLIENTS, MAX_DNS, MAX_STATIC, MAX_POOLS>
{
    /// Where a reply goes (RFC 2131 section 4.1): to the relay agent if the
    /// request came through one, to the client's own address if it has one,
    /// otherwise broadcast.
    pub(super) const fn destination(message: &ClientMessage, msg_type: u8) -> (Ipv4Addr, u16) {
        match (message.giaddr, message.ciaddr) {
            (Some(relay), _) => (relay, DHCP_SERVER_PORT),
            (None, Some(ciaddr)) if msg_type != DHCP_NAK => (ciaddr, DHCP_CLIENT_PORT),
            _ => (Ipv4Addr::BROADCAST, DHCP_CLIENT_PORT),
        }
    }

    /// Build a reply of `msg_type` to `message`, assigning `yiaddr`.
    pub(super) fn reply(
        &self,
        message: &ClientMessage,
        msg_type: u8,
        yiaddr: Option<Ipv4Addr>,
        subnet: &Subnet,
    ) -> (Vec<u8, DHCP_PACKET_SIZE>, (Ipv4Addr, u16)) {
        let request = &message.packet;
        let mut flags = u16::from_be(request.flags);
        if msg_type == DHCP_NAK && message.giaddr.is_some() {
            // A relay must broadcast a NAK: the client's address is unusable.
            flags |= BROADCAST_FLAG;
        }
        let mut resp = DhcpPacket {
            op: BOOTREPLY,
            htype: HTYPE_ETHERNET,
            hlen: HLEN_ETHERNET,
            xid: request.xid,
            flags: flags.to_be(),
            giaddr: request.giaddr,
            magic: DHCP_MAGIC,
            ..Default::default()
        };
        if msg_type == DHCP_ACK {
            resp.ciaddr = request.ciaddr;
        }
        resp.yiaddr = yiaddr.unwrap_or(Ipv4Addr::UNSPECIFIED).octets();
        resp.chaddr = request.chaddr;

        let mut bytes = Vec::<u8, DHCP_PACKET_SIZE>::new();
        // Safe serialization of a packed struct: transmute to a byte array
        // avoids alignment issues that from_raw_parts would have.
        let resp_bytes: [u8; FIXED_PART_SIZE] = unsafe { core::mem::transmute(resp) };
        bytes.extend_from_slice(&resp_bytes).ok();
        self.add_options(&mut bytes, msg_type, yiaddr.is_some(), subnet);
        (bytes, Self::destination(message, msg_type))
    }

    /// Appends the options of a reply. `assigns` is `false` for a NAK and for
    /// the ACK to a `DHCPINFORM`, which carry no lease.
    fn add_options(
        &self,
        packet: &mut Vec<u8, DHCP_PACKET_SIZE>,
        msg_type: u8,
        assigns: bool,
        subnet: &Subnet,
    ) {
        // Each option is written whole or not at all, with room kept for END.
        let mut push = |code: u8, value: &[u8]| {
            let Ok(len) = u8::try_from(value.len()) else {
                return;
            };
            if packet.len() + 2 + value.len() < packet.capacity() {
                let _ = packet.extend_from_slice(&[code, len]);
                let _ = packet.extend_from_slice(value);
            }
        };

        push(options::MESSAGE_TYPE, &[msg_type]);
        push(options::SERVER_ID, &self.config.server_ip.octets());
        if msg_type != DHCP_NAK {
            if assigns {
                let lease = self.config.lease_time;
                push(options::LEASE_TIME, &lease.to_be_bytes());
                if lease != u32::MAX {
                    // T1 at 50% and T2 at 87.5% of the lease (RFC 2131 section 4.4.5).
                    push(options::RENEWAL_TIME, &(lease / 2).to_be_bytes());
                    push(options::REBINDING_TIME, &(lease - lease / 8).to_be_bytes());
                }
            }
            push(options::SUBNET_MASK, &subnet.mask.to_be_bytes());
            if let Some(router) = subnet.router {
                push(options::ROUTER, &router.octets());
            }
            push(options::BROADCAST_ADDRESS, &subnet.broadcast().octets());
            if !self.config.dns_servers.is_empty() {
                // An option holds at most 255 bytes: 63 addresses.
                let mut dns = Vec::<u8, 252>::new();
                for server in self.config.dns_servers.iter().take(63) {
                    let _ = dns.extend_from_slice(&server.octets());
                }
                push(options::DNS_SERVER, &dns);
            }
            if let Some(domain) = self.config.domain_name {
                push(options::DOMAIN_NAME, domain.as_bytes());
            }
        }
        let _ = packet.push(options::END);
    }
}
