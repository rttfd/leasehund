use super::*;
use core::net::Ipv4Addr;

type TestServer = DhcpServer<4, 2, 2>;
type TestBuilder = DhcpConfigBuilder<2>;

const SERVER: Ipv4Addr = Ipv4Addr::new(10, 0, 0, 1);
const MAC_A: [u8; 6] = [0x02, 0, 0, 0, 0, 0xA];
const MAC_B: [u8; 6] = [0x02, 0, 0, 0, 0, 0xB];
const LEASE_MS: u64 = 3_600_000;

const fn ip(last: u8) -> Ipv4Addr {
    Ipv4Addr::new(10, 0, 0, last)
}

/// Server on 10.0.0.0/24 handing out 10.0.0.100..=10.0.0.103 for one hour.
fn server() -> TestServer {
    let config = TestBuilder::new()
        .server_ip(SERVER)
        .subnet_mask(Ipv4Addr::new(255, 255, 255, 0))
        .router(SERVER)
        .add_dns_server(Ipv4Addr::new(1, 1, 1, 1))
        .ip_pool(ip(100), ip(103))
        .lease_time(3600)
        .build();
    TestServer::with_config(config)
}

/// Builds a client message.
struct Msg {
    packet: DhcpPacket,
    options: heapless::Vec<u8, 64>,
}

impl Msg {
    fn new(msg_type: u8, mac: [u8; 6]) -> Self {
        let mut packet = DhcpPacket {
            op: BOOTREQUEST,
            htype: HTYPE_ETHERNET,
            hlen: HLEN_ETHERNET,
            xid: 0x1234_5678_u32.to_be(),
            ..Default::default()
        };
        packet.chaddr[..6].copy_from_slice(&mac);
        let mut options = heapless::Vec::new();
        options
            .extend_from_slice(&[options::MESSAGE_TYPE, 1, msg_type])
            .unwrap();
        Self { packet, options }
    }

    fn ip_option(mut self, code: u8, value: Ipv4Addr) -> Self {
        self.options.extend_from_slice(&[code, 4]).unwrap();
        self.options.extend_from_slice(&value.octets()).unwrap();
        self
    }

    fn requested(self, value: Ipv4Addr) -> Self {
        self.ip_option(options::REQUESTED_IP, value)
    }

    fn server_id(self, value: Ipv4Addr) -> Self {
        self.ip_option(options::SERVER_ID, value)
    }

    const fn ciaddr(mut self, value: Ipv4Addr) -> Self {
        self.packet.ciaddr = value.octets();
        self
    }

    const fn giaddr(mut self, value: Ipv4Addr) -> Self {
        self.packet.giaddr = value.octets();
        self
    }

    fn hostname(mut self, name: &[u8]) -> Self {
        self.options
            .extend_from_slice(&[options::HOST_NAME, u8::try_from(name.len()).unwrap()])
            .unwrap();
        self.options.extend_from_slice(name).unwrap();
        self
    }

    const fn broadcast_flag(mut self) -> Self {
        self.packet.flags = BROADCAST_FLAG.to_be();
        self
    }

    fn bytes(&self) -> heapless::Vec<u8, 600> {
        let fixed: [u8; FIXED_PART_SIZE] = unsafe { core::mem::transmute(self.packet) };
        let mut data = heapless::Vec::new();
        data.extend_from_slice(&fixed).unwrap();
        data.extend_from_slice(&self.options).unwrap();
        data.push(options::END).unwrap();
        data
    }
}

/// A decoded server reply.
struct Reply {
    bytes: heapless::Vec<u8, DHCP_PACKET_SIZE>,
    to: (Ipv4Addr, u16),
}

impl Reply {
    fn field(&self, at: usize) -> Ipv4Addr {
        Ipv4Addr::new(
            self.bytes[at],
            self.bytes[at + 1],
            self.bytes[at + 2],
            self.bytes[at + 3],
        )
    }

    fn yiaddr(&self) -> Ipv4Addr {
        self.field(16)
    }

    fn ciaddr(&self) -> Ipv4Addr {
        self.field(12)
    }

    fn giaddr(&self) -> Ipv4Addr {
        self.field(24)
    }

    fn flags(&self) -> u16 {
        u16::from_be_bytes([self.bytes[10], self.bytes[11]])
    }

    fn options(&self) -> &[u8] {
        &self.bytes[FIXED_PART_SIZE..]
    }

    fn msg_type(&self) -> Option<u8> {
        options::message_type(self.options())
    }

    fn option(&self, code: u8) -> Option<&[u8]> {
        options::find(self.options(), code)
    }

    fn u32_option(&self, code: u8) -> Option<u32> {
        Some(u32::from_be_bytes(self.option(code)?.try_into().ok()?))
    }
}

fn process_filtered(
    server: &mut TestServer,
    msg: &Msg,
    now: u64,
    allow: &mut impl FnMut([u8; 6], AdmissionEvent) -> bool,
) -> (Option<Reply>, Option<TransactionEvent>) {
    let action = server.prepare_packet_with_filter(&msg.bytes(), allow, now);
    let reply = action.response.map(|(bytes, to)| Reply { bytes, to });
    (reply, action.event.map(|(event, _)| event))
}

fn process(
    server: &mut TestServer,
    msg: &Msg,
    now: u64,
) -> (Option<Reply>, Option<TransactionEvent>) {
    process_filtered(server, msg, now, &mut |_, _| true)
}

/// Run DISCOVER and REQUEST for `mac`, returning the leased address.
fn lease(server: &mut TestServer, mac: [u8; 6], now: u64) -> Ipv4Addr {
    let (offer, _) = process(server, &Msg::new(DHCP_DISCOVER, mac), now);
    let offered = offer.expect("offer").yiaddr();
    let request = Msg::new(DHCP_REQUEST, mac)
        .server_id(SERVER)
        .requested(offered);
    let (ack, event) = process(server, &request, now);
    assert_eq!(ack.expect("ack").msg_type(), Some(DHCP_ACK));
    assert_eq!(event, Some(TransactionEvent::Leased(offered, mac)));
    offered
}

// ---------------------------------------------------------------- DORA

#[test]
fn discover_offer_request_ack() {
    let mut server = server();

    let (offer, event) = process(&mut server, &Msg::new(DHCP_DISCOVER, MAC_A), 0);
    let offer = offer.unwrap();
    assert_eq!(event, None);
    assert_eq!(offer.msg_type(), Some(DHCP_OFFER));
    assert_eq!(offer.yiaddr(), ip(100));
    assert_eq!(offer.to, (Ipv4Addr::BROADCAST, DHCP_CLIENT_PORT));
    assert_eq!(&offer.bytes[28..34], &MAC_A);
    assert_eq!(&offer.bytes[4..8], &0x1234_5678_u32.to_be_bytes());
    // Reserved for the offer, not yet bound.
    let reserved = server.lease_for(MAC_A).unwrap();
    assert!(!reserved.bound);
    assert_eq!(reserved.expires_at_ms, OFFER_RESERVATION_MS);

    let request = Msg::new(DHCP_REQUEST, MAC_A)
        .server_id(SERVER)
        .requested(ip(100));
    let (ack, event) = process(&mut server, &request, 10);
    let ack = ack.unwrap();
    assert_eq!(ack.msg_type(), Some(DHCP_ACK));
    assert_eq!(ack.yiaddr(), ip(100));
    assert_eq!(event, Some(TransactionEvent::Leased(ip(100), MAC_A)));
    let bound = server.lease_for(MAC_A).unwrap();
    assert!(bound.bound);
    assert_eq!(bound.expires_at_ms, 10 + LEASE_MS);
}

#[test]
fn ack_carries_configuration_and_renewal_times() {
    let mut server = server();
    lease(&mut server, MAC_A, 0);
    let request = Msg::new(DHCP_REQUEST, MAC_A).ciaddr(ip(100));
    let (ack, _) = process(&mut server, &request, 0);
    let ack = ack.unwrap();

    assert_eq!(
        options::find_ipv4(ack.options(), options::SERVER_ID),
        Some(SERVER)
    );
    assert_eq!(ack.u32_option(options::LEASE_TIME), Some(3600));
    assert_eq!(ack.u32_option(options::RENEWAL_TIME), Some(1800));
    assert_eq!(ack.u32_option(options::REBINDING_TIME), Some(3150));
    assert_eq!(
        options::find_ipv4(ack.options(), options::SUBNET_MASK),
        Some(Ipv4Addr::new(255, 255, 255, 0))
    );
    assert_eq!(
        options::find_ipv4(ack.options(), options::ROUTER),
        Some(SERVER)
    );
    assert_eq!(
        options::find_ipv4(ack.options(), options::BROADCAST_ADDRESS),
        Some(ip(255))
    );
    assert_eq!(ack.option(options::DNS_SERVER), Some(&[1u8, 1, 1, 1][..]));
    assert_eq!(ack.option(options::DOMAIN_NAME), None);
    // The option field ends with END.
    assert_eq!(ack.bytes.last(), Some(&options::END));
}

#[test]
fn domain_name_and_infinite_lease() {
    let config = TestBuilder::new()
        .server_ip(SERVER)
        .ip_pool(ip(100), ip(103))
        .subnet_mask(Ipv4Addr::new(255, 255, 255, 0))
        .lease_time(u32::MAX)
        .domain_name("lan")
        .no_router()
        .build();
    let mut server = TestServer::with_config(config);
    lease(&mut server, MAC_A, 5);

    let (ack, _) = process(
        &mut server,
        &Msg::new(DHCP_REQUEST, MAC_A).ciaddr(ip(100)),
        5,
    );
    let ack = ack.unwrap();
    assert_eq!(ack.option(options::DOMAIN_NAME), Some(&b"lan"[..]));
    assert_eq!(ack.u32_option(options::LEASE_TIME), Some(u32::MAX));
    // No renewal times for a lease that never expires.
    assert_eq!(ack.option(options::RENEWAL_TIME), None);
    assert_eq!(ack.option(options::REBINDING_TIME), None);
    assert_eq!(server.lease_for(MAC_A).unwrap().expires_at_ms, u64::MAX);
    // Without a router, no router option.
    assert_eq!(ack.option(options::ROUTER), None);
}

#[test]
fn dns_option_holds_at_most_63_servers() {
    let mut builder = DhcpConfigBuilder::<64>::new()
        .server_ip(SERVER)
        .ip_pool(ip(100), ip(103));
    for n in 0..64 {
        builder = builder.add_dns_server(Ipv4Addr::new(9, 9, 9, n));
    }
    let mut server = DhcpServer::<4, 64, 2>::with_config(builder.build());
    let action = server.prepare_packet_with_filter(
        &Msg::new(DHCP_DISCOVER, MAC_A).bytes(),
        &mut |_, _| true,
        0,
    );
    let (bytes, _) = action.response.unwrap();
    let dns = options::find(&bytes[FIXED_PART_SIZE..], options::DNS_SERVER).unwrap();
    assert_eq!(dns.len(), 252);
}

// ---------------------------------------------------------------- allocation

#[test]
fn rediscover_keeps_the_address_and_the_full_lease() {
    let mut server = server();
    let address = lease(&mut server, MAC_A, 0);
    let before = server.lease_for(MAC_A).unwrap();

    let (offer, _) = process(&mut server, &Msg::new(DHCP_DISCOVER, MAC_A), 1_000);
    assert_eq!(offer.unwrap().yiaddr(), address);
    // The bound lease was not downgraded to a 60 second reservation.
    assert_eq!(server.lease_for(MAC_A).unwrap(), before);
}

#[test]
fn offers_do_not_collide() {
    let mut server = server();
    let (a, _) = process(&mut server, &Msg::new(DHCP_DISCOVER, MAC_A), 0);
    let (b, _) = process(&mut server, &Msg::new(DHCP_DISCOVER, MAC_B), 0);
    assert_eq!(a.unwrap().yiaddr(), ip(100));
    assert_eq!(b.unwrap().yiaddr(), ip(101));
    // Repeating a discover repeats the same offer.
    let (again, _) = process(&mut server, &Msg::new(DHCP_DISCOVER, MAC_A), 1);
    assert_eq!(again.unwrap().yiaddr(), ip(100));
}

#[test]
fn requested_address_is_offered_when_free() {
    let mut server = server();
    let (offer, _) = process(
        &mut server,
        &Msg::new(DHCP_DISCOVER, MAC_A).requested(ip(102)),
        0,
    );
    assert_eq!(offer.unwrap().yiaddr(), ip(102));
    // Not when it is outside the pool or taken.
    let (offer, _) = process(
        &mut server,
        &Msg::new(DHCP_DISCOVER, MAC_B).requested(ip(50)),
        0,
    );
    assert_eq!(offer.unwrap().yiaddr(), ip(100));
}

#[test]
fn exhausted_pool_offers_nothing() {
    let mut server = server();
    for n in 0..4u8 {
        lease(&mut server, [2, 0, 0, 0, 1, n], 0);
    }
    assert!(server.is_pool_full());
    let (offer, _) = process(&mut server, &Msg::new(DHCP_DISCOVER, MAC_A), 0);
    // Previously an offer of 0.0.0.0 went out.
    assert!(offer.is_none());
    assert!(server.lease_for(MAC_A).is_none());
}

#[test]
fn full_lease_table_offers_nothing() {
    // Room for 2 leases, 4 addresses.
    let config = TestBuilder::new()
        .server_ip(SERVER)
        .subnet_mask(Ipv4Addr::new(255, 255, 255, 0))
        .ip_pool(ip(100), ip(103))
        .build();
    let mut server = DhcpServer::<2, 2, 2>::with_config(config);
    for mac in [MAC_A, MAC_B] {
        let action = server.prepare_packet_with_filter(
            &Msg::new(DHCP_DISCOVER, mac).bytes(),
            &mut |_, _| true,
            0,
        );
        assert!(action.response.is_some());
    }
    assert!(server.is_pool_full());
    let action = server.prepare_packet_with_filter(
        &Msg::new(DHCP_DISCOVER, [2, 0, 0, 0, 0, 0xC]).bytes(),
        &mut |_, _| true,
        0,
    );
    // An unrecorded offer could be made twice.
    assert!(action.response.is_none());
}

#[test]
fn infrastructure_addresses_are_never_assigned() {
    // A pool covering the whole /29, including the network (.0), the server
    // (.1), the router (.2) and the broadcast address (.7).
    let config = TestBuilder::new()
        .server_ip(ip(1))
        .router(ip(2))
        .subnet_mask(Ipv4Addr::new(255, 255, 255, 248))
        .ip_pool(ip(0), ip(7))
        .build();
    let mut server = DhcpServer::<8, 2, 2>::with_config(config);
    let mut offered = heapless::Vec::<Ipv4Addr, 8>::new();
    for n in 0..8u8 {
        let mac = [2, 0, 0, 0, 2, n];
        let action = server.prepare_packet_with_filter(
            &Msg::new(DHCP_DISCOVER, mac).bytes(),
            &mut |_, _| true,
            0,
        );
        if let Some((bytes, _)) = action.response {
            offered
                .push(Ipv4Addr::new(bytes[16], bytes[17], bytes[18], bytes[19]))
                .unwrap();
        }
    }
    assert_eq!(offered.as_slice(), [ip(3), ip(4), ip(5), ip(6)]);
}

#[test]
fn inverted_pool_does_not_panic() {
    let config = TestBuilder::new()
        .server_ip(SERVER)
        .ip_pool(ip(103), ip(100))
        .build();
    let server = TestServer::with_config(config.clone());
    assert_eq!(server.get_next_available_ip(), None);
    assert!(server.is_pool_full());
    assert_eq!(config.validate(), Err(ConfigError::PoolStartAfterEnd));
}

#[test]
fn offer_reservation_expires() {
    let mut server = server();
    process(&mut server, &Msg::new(DHCP_DISCOVER, MAC_A), 0);
    // While reserved, B gets another address.
    let (offer, _) = process(
        &mut server,
        &Msg::new(DHCP_DISCOVER, MAC_B).requested(ip(100)),
        1,
    );
    assert_eq!(offer.unwrap().yiaddr(), ip(101));
    // After the reservation lapsed, A's address is free again.
    let later = OFFER_RESERVATION_MS + 1;
    let (offer, _) = process(
        &mut server,
        &Msg::new(DHCP_DISCOVER, [2, 0, 0, 0, 0, 0xC]),
        later,
    );
    assert_eq!(offer.unwrap().yiaddr(), ip(100));
}

#[test]
fn bound_lease_expires() {
    let mut server = server();
    lease(&mut server, MAC_A, 0);
    server.purge_expired_at(LEASE_MS - 1);
    assert_eq!(server.lease_count(), 1);
    server.purge_expired_at(LEASE_MS);
    assert_eq!(server.lease_count(), 0);
}

// ---------------------------------------------------------------- requests

#[test]
fn request_for_another_server_frees_our_offer() {
    let mut server = server();
    process(&mut server, &Msg::new(DHCP_DISCOVER, MAC_A), 0);
    assert!(server.lease_for(MAC_A).is_some());

    let elsewhere = Msg::new(DHCP_REQUEST, MAC_A)
        .server_id(Ipv4Addr::new(10, 0, 0, 2))
        .requested(ip(100));
    let (reply, event) = process(&mut server, &elsewhere, 1);
    assert!(reply.is_none());
    assert_eq!(event, None);
    // The reservation was released at once.
    assert!(server.lease_for(MAC_A).is_none());
}

#[test]
fn request_for_another_server_keeps_a_bound_lease() {
    let mut server = server();
    lease(&mut server, MAC_A, 0);
    let elsewhere = Msg::new(DHCP_REQUEST, MAC_A)
        .server_id(Ipv4Addr::new(10, 0, 0, 2))
        .requested(ip(100));
    let (reply, _) = process(&mut server, &elsewhere, 1);
    assert!(reply.is_none());
    assert!(server.lease_for(MAC_A).is_some());
}

#[test]
fn selecting_request_with_wrong_or_missing_address_gets_nak() {
    let mut server = server();
    process(&mut server, &Msg::new(DHCP_DISCOVER, MAC_A), 0);

    let wrong = Msg::new(DHCP_REQUEST, MAC_A)
        .server_id(SERVER)
        .requested(ip(103));
    let (nak, event) = process(&mut server, &wrong, 1);
    let nak = nak.unwrap();
    assert_eq!(nak.msg_type(), Some(DHCP_NAK));
    assert_eq!(nak.yiaddr(), Ipv4Addr::UNSPECIFIED);
    assert_eq!(nak.to, (Ipv4Addr::BROADCAST, DHCP_CLIENT_PORT));
    // A NAK carries no configuration.
    assert_eq!(nak.option(options::LEASE_TIME), None);
    assert_eq!(nak.option(options::SUBNET_MASK), None);
    assert_eq!(event, None);

    let missing = Msg::new(DHCP_REQUEST, MAC_A).server_id(SERVER);
    let (nak, _) = process(&mut server, &missing, 2);
    assert_eq!(nak.unwrap().msg_type(), Some(DHCP_NAK));
}

#[test]
fn init_reboot_after_server_restart() {
    // A fresh server knows no leases, as after a reboot.
    let mut server = server();

    let free = Msg::new(DHCP_REQUEST, MAC_A).requested(ip(102));
    let (ack, event) = process(&mut server, &free, 0);
    assert_eq!(ack.unwrap().yiaddr(), ip(102));
    assert_eq!(event, Some(TransactionEvent::Leased(ip(102), MAC_A)));

    let taken = Msg::new(DHCP_REQUEST, MAC_B).requested(ip(102));
    let (nak, _) = process(&mut server, &taken, 0);
    assert_eq!(nak.unwrap().msg_type(), Some(DHCP_NAK));

    let elsewhere = Msg::new(DHCP_REQUEST, MAC_B).requested(Ipv4Addr::new(192, 168, 5, 5));
    let (nak, _) = process(&mut server, &elsewhere, 0);
    assert_eq!(nak.unwrap().msg_type(), Some(DHCP_NAK));
}

#[test]
fn init_reboot_with_a_different_address_than_the_lease_gets_nak() {
    let mut server = server();
    lease(&mut server, MAC_A, 0);
    let other = Msg::new(DHCP_REQUEST, MAC_A).requested(ip(103));
    let (nak, _) = process(&mut server, &other, 1);
    assert_eq!(nak.unwrap().msg_type(), Some(DHCP_NAK));
}

#[test]
fn renewal_is_unicast_to_the_client() {
    let mut server = server();
    let address = lease(&mut server, MAC_A, 0);
    let renew = Msg::new(DHCP_REQUEST, MAC_A).ciaddr(address);
    let (ack, event) = process(&mut server, &renew, 1_000);
    let ack = ack.unwrap();
    assert_eq!(ack.msg_type(), Some(DHCP_ACK));
    assert_eq!(ack.to, (address, DHCP_CLIENT_PORT));
    assert_eq!(ack.ciaddr(), address);
    assert_eq!(event, Some(TransactionEvent::Leased(address, MAC_A)));
    assert_eq!(
        server.lease_for(MAC_A).unwrap().expires_at_ms,
        1_000 + LEASE_MS
    );
}

#[test]
fn request_without_any_address_is_ignored() {
    let mut server = server();
    let (reply, event) = process(&mut server, &Msg::new(DHCP_REQUEST, MAC_A), 0);
    // Previously this was ACKed with whatever address was next, or 0.0.0.0.
    assert!(reply.is_none());
    assert_eq!(event, None);
    assert_eq!(server.lease_count(), 0);
}

#[test]
fn full_lease_table_naks_instead_of_an_unrecorded_ack() {
    let config = TestBuilder::new()
        .server_ip(SERVER)
        .subnet_mask(Ipv4Addr::new(255, 255, 255, 0))
        .ip_pool(ip(100), ip(103))
        .build();
    let mut server = DhcpServer::<2, 2, 2>::with_config(config);
    for (mac, address) in [(MAC_A, ip(100)), (MAC_B, ip(101))] {
        let action = server.prepare_packet_with_filter(
            &Msg::new(DHCP_REQUEST, mac).requested(address).bytes(),
            &mut |_, _| true,
            0,
        );
        assert!(action.event.is_some());
    }
    let action = server.prepare_packet_with_filter(
        &Msg::new(DHCP_REQUEST, [2, 0, 0, 0, 0, 0xC])
            .requested(ip(102))
            .bytes(),
        &mut |_, _| true,
        0,
    );
    let (bytes, _) = action.response.unwrap();
    assert_eq!(
        options::message_type(&bytes[FIXED_PART_SIZE..]),
        Some(DHCP_NAK)
    );
    assert_eq!(action.event, None);
}

// ---------------------------------------------------------------- addressing

#[test]
fn relayed_requests_are_answered_through_the_relay() {
    let mut server = server();
    let relay = Ipv4Addr::new(10, 0, 0, 254);
    let (offer, _) = process(
        &mut server,
        &Msg::new(DHCP_DISCOVER, MAC_A).giaddr(relay),
        0,
    );
    let offer = offer.unwrap();
    assert_eq!(offer.to, (relay, DHCP_SERVER_PORT));
    assert_eq!(offer.giaddr(), relay);

    let wrong = Msg::new(DHCP_REQUEST, MAC_A)
        .giaddr(relay)
        .server_id(SERVER)
        .requested(ip(103));
    let (nak, _) = process(&mut server, &wrong, 0);
    let nak = nak.unwrap();
    assert_eq!(nak.to, (relay, DHCP_SERVER_PORT));
    // The relay must broadcast the NAK to the client.
    assert_eq!(nak.flags() & BROADCAST_FLAG, BROADCAST_FLAG);
}

#[test]
fn broadcast_flag_is_echoed() {
    let mut server = server();
    let (offer, _) = process(
        &mut server,
        &Msg::new(DHCP_DISCOVER, MAC_A).broadcast_flag(),
        0,
    );
    assert_eq!(offer.unwrap().flags(), BROADCAST_FLAG);
    let (offer, _) = process(&mut server, &Msg::new(DHCP_DISCOVER, MAC_B), 0);
    assert_eq!(offer.unwrap().flags(), 0);
}

// ---------------------------------------------------------------- decline, release, inform

#[test]
fn declined_address_is_withheld() {
    let mut server = server();
    let address = lease(&mut server, MAC_A, 0);
    let decline = Msg::new(DHCP_DECLINE, MAC_A)
        .server_id(SERVER)
        .requested(address);
    let (reply, event) = process(&mut server, &decline, 10);
    assert!(reply.is_none());
    assert_eq!(event, None);
    assert!(server.lease_for(MAC_A).is_none());

    // Nobody gets the conflicting address while it is held.
    let (offer, _) = process(&mut server, &Msg::new(DHCP_DISCOVER, MAC_A), 20);
    assert_eq!(offer.unwrap().yiaddr(), ip(101));
    let (nak, _) = process(
        &mut server,
        &Msg::new(DHCP_REQUEST, MAC_B).requested(address),
        20,
    );
    assert_eq!(nak.unwrap().msg_type(), Some(DHCP_NAK));

    // After the hold it is usable again.
    let later = 10 + DECLINE_HOLD_MS;
    let (offer, _) = process(&mut server, &Msg::new(DHCP_DISCOVER, MAC_B), later);
    assert_eq!(offer.unwrap().yiaddr(), address);
}

#[test]
fn decline_for_another_server_is_ignored() {
    let mut server = server();
    let address = lease(&mut server, MAC_A, 0);
    let decline = Msg::new(DHCP_DECLINE, MAC_A)
        .server_id(Ipv4Addr::new(10, 0, 0, 2))
        .requested(address);
    process(&mut server, &decline, 1);
    assert!(server.lease_for(MAC_A).is_some());
}

#[test]
fn release_frees_the_matching_lease() {
    let mut server = server();
    let address = lease(&mut server, MAC_A, 0);

    let wrong_ip = Msg::new(DHCP_RELEASE, MAC_A)
        .server_id(SERVER)
        .ciaddr(ip(103));
    assert_eq!(process(&mut server, &wrong_ip, 1).1, None);
    let other_server = Msg::new(DHCP_RELEASE, MAC_A)
        .server_id(Ipv4Addr::new(10, 0, 0, 2))
        .ciaddr(address);
    assert_eq!(process(&mut server, &other_server, 1).1, None);
    assert!(server.lease_for(MAC_A).is_some());

    let release = Msg::new(DHCP_RELEASE, MAC_A)
        .server_id(SERVER)
        .ciaddr(address);
    let (reply, event) = process(&mut server, &release, 1);
    assert!(reply.is_none());
    assert_eq!(event, Some(TransactionEvent::Released(address, MAC_A)));
    assert!(server.lease_for(MAC_A).is_none());
}

#[test]
fn inform_gets_configuration_without_a_lease() {
    let mut server = server();
    let client = Ipv4Addr::new(10, 0, 0, 50);
    let mut seen = None;
    let (ack, event) = process_filtered(
        &mut server,
        &Msg::new(DHCP_INFORM, MAC_A).ciaddr(client),
        0,
        &mut |_, kind| {
            seen = Some(kind);
            true
        },
    );
    let ack = ack.unwrap();
    assert_eq!(seen, Some(AdmissionEvent::Inform));
    assert_eq!(ack.msg_type(), Some(DHCP_ACK));
    assert_eq!(ack.yiaddr(), Ipv4Addr::UNSPECIFIED);
    assert_eq!(ack.to, (client, DHCP_CLIENT_PORT));
    assert_eq!(ack.option(options::LEASE_TIME), None);
    assert!(ack.option(options::DNS_SERVER).is_some());
    assert_eq!(event, None);
    assert_eq!(server.lease_count(), 0);
}

// ---------------------------------------------------------------- static leases

#[test]
fn static_lease_is_always_offered_and_reserved() {
    let mut server = server();
    // Outside the dynamic pool is fine.
    server.add_static_lease(MAC_A, ip(20)).unwrap();
    server.add_static_lease(MAC_B, ip(100)).unwrap();

    assert_eq!(lease(&mut server, MAC_A, 0), ip(20));
    // .100 belongs to B, so the next client skips it.
    let other = [2, 0, 0, 0, 0, 0xC];
    assert_eq!(lease(&mut server, other, 0), ip(101));
    assert_eq!(lease(&mut server, MAC_B, 0), ip(100));
    // Nobody else may claim a static address.
    let (nak, _) = process(
        &mut server,
        &Msg::new(DHCP_REQUEST, other).requested(ip(20)),
        0,
    );
    assert_eq!(nak.unwrap().msg_type(), Some(DHCP_NAK));
    // And the static client cannot take a different one.
    let (nak, _) = process(
        &mut server,
        &Msg::new(DHCP_REQUEST, MAC_A).requested(ip(102)),
        0,
    );
    assert_eq!(nak.unwrap().msg_type(), Some(DHCP_NAK));
}

#[test]
fn static_lease_errors_and_removal() {
    let mut server = server();
    assert_eq!(
        server.add_static_lease(MAC_A, SERVER),
        Err(StaticLeaseError::ReservedAddress)
    );
    assert_eq!(
        server.add_static_lease(MAC_A, ip(255)),
        Err(StaticLeaseError::ReservedAddress)
    );
    server.add_static_lease(MAC_A, ip(20)).unwrap();
    assert_eq!(
        server.add_static_lease(MAC_B, ip(20)),
        Err(StaticLeaseError::AddressTaken)
    );
    // Replacing the address of an existing entry does not need a new slot.
    server.add_static_lease(MAC_A, ip(21)).unwrap();
    server.add_static_lease(MAC_B, ip(22)).unwrap();
    assert_eq!(
        server.add_static_lease([2, 0, 0, 0, 0, 0xC], ip(23)),
        Err(StaticLeaseError::Full)
    );
    assert_eq!(server.remove_static_lease(MAC_A), Some(ip(21)));
    assert_eq!(server.remove_static_lease(MAC_A), None);
}

// ---------------------------------------------------------------- lease table API

#[test]
fn leases_lease_for_and_revoke() {
    let mut server = server();
    lease(&mut server, MAC_A, 0);
    process(&mut server, &Msg::new(DHCP_DISCOVER, MAC_B), 0);

    let mut all: heapless::Vec<Lease, 4> = server.leases().collect();
    all.sort_unstable_by_key(|lease| lease.ip);
    assert_eq!(all.len(), 2);
    assert!(all[0].bound && all[0].mac == MAC_A);
    assert!(!all[1].bound && all[1].mac == MAC_B);

    assert_eq!(server.revoke(MAC_A), Some(ip(100)));
    assert_eq!(server.revoke(MAC_A), None);
    assert_eq!(server.lease_count(), 1);
}

// ---------------------------------------------------------------- parsing

#[test]
fn pad_before_message_type_is_understood() {
    let mut server = server();
    let mut msg = Msg::new(DHCP_DISCOVER, MAC_A);
    msg.options.clear();
    msg.options
        .extend_from_slice(&[
            options::PAD,
            options::PAD,
            options::MESSAGE_TYPE,
            1,
            DHCP_DISCOVER,
        ])
        .unwrap();
    let (offer, _) = process(&mut server, &msg, 0);
    assert_eq!(offer.unwrap().msg_type(), Some(DHCP_OFFER));
}

#[test]
fn non_request_and_malformed_packets_are_ignored() {
    let mut server = server();

    let mut reply_packet = Msg::new(DHCP_DISCOVER, MAC_A);
    reply_packet.packet.op = BOOTREPLY;
    let mut token_ring = Msg::new(DHCP_DISCOVER, MAC_A);
    token_ring.packet.htype = 6;
    let mut long_hw = Msg::new(DHCP_DISCOVER, MAC_A);
    long_hw.packet.hlen = 16;
    let mut bad_magic = Msg::new(DHCP_DISCOVER, MAC_A);
    bad_magic.packet.magic = [0; 4];
    let mut no_type = Msg::new(DHCP_DISCOVER, MAC_A);
    no_type.options.clear();

    for msg in [reply_packet, token_ring, long_hw, bad_magic, no_type] {
        assert!(process(&mut server, &msg, 0).0.is_none());
    }
    let short = &Msg::new(DHCP_DISCOVER, MAC_A).bytes()[..100];
    assert!(
        server
            .prepare_packet_with_filter(short, &mut |_, _| true, 0)
            .response
            .is_none()
    );
    assert_eq!(server.lease_count(), 0);
}

// ---------------------------------------------------------------- admission

#[test]
fn denied_discover_produces_no_response_or_lease() {
    let mut server = server();
    let mut deny = |received_mac, event| {
        assert_eq!(received_mac, MAC_A);
        assert_eq!(event, AdmissionEvent::Discover);
        false
    };
    let (reply, _) = process_filtered(&mut server, &Msg::new(DHCP_DISCOVER, MAC_A), 0, &mut deny);
    assert!(reply.is_none());
    assert_eq!(server.lease_count(), 0);
}

#[test]
fn denied_request_produces_no_response_or_lease() {
    let mut server = server();
    let mut deny = |received_mac, event| {
        assert_eq!(received_mac, MAC_A);
        assert_eq!(event, AdmissionEvent::Request);
        false
    };
    let request = Msg::new(DHCP_REQUEST, MAC_A).requested(ip(100));
    let (reply, _) = process_filtered(&mut server, &request, 0, &mut deny);
    assert!(reply.is_none());
    assert_eq!(server.lease_count(), 0);
}

#[test]
fn releases_ignore_the_admission_policy() {
    let mut server = server();
    let address = lease(&mut server, MAC_A, 0);
    let release = Msg::new(DHCP_RELEASE, MAC_A).ciaddr(address);
    let (_, event) = process_filtered(&mut server, &release, 1, &mut |_, _| false);
    assert_eq!(event, Some(TransactionEvent::Released(address, MAC_A)));
}

// ---------------------------------------------------------------- configuration

#[test]
fn config_builder_basic() {
    let config = TestBuilder::new()
        .server_ip(ip(1))
        .subnet_mask(Ipv4Addr::new(255, 255, 255, 0))
        .router(ip(254))
        .add_dns_server(Ipv4Addr::new(8, 8, 8, 8))
        .ip_pool(ip(100), ip(200))
        .lease_time(3600)
        .domain_name("home.arpa")
        .build();
    assert_eq!(config.server_ip, ip(1));
    assert_eq!(config.router, Some(ip(254)));
    assert_eq!(config.dns_servers.as_slice(), [Ipv4Addr::new(8, 8, 8, 8)]);
    assert_eq!(config.ip_pool_start, ip(100));
    assert_eq!(config.ip_pool_end, ip(200));
    assert_eq!(config.lease_time, 3600);
    assert_eq!(config.domain_name, Some("home.arpa"));
}

#[test]
fn config_builder_defaults() {
    let config = TestBuilder::new().build();
    assert!(config.dns_servers.is_empty());
    assert_eq!(config.domain_name, None);
    assert_eq!(TestBuilder::new().no_router().build().router, None);

    let config = DhcpConfig::<2>::default();
    assert_eq!(config.server_ip, Ipv4Addr::new(192, 168, 1, 1));
    assert_eq!(config.dns_servers.as_slice(), [Ipv4Addr::new(8, 8, 8, 8)]);
    assert_eq!(config.lease_time, DEFAULT_LEASE_TIME);
    assert_eq!(config.validate(), Ok(()));
}

#[test]
fn config_validation() {
    static LONG: [u8; 256] = [b'x'; 256];
    let base = || {
        TestBuilder::new()
            .server_ip(ip(1))
            .subnet_mask(Ipv4Addr::new(255, 255, 255, 0))
            .no_router()
            .ip_pool(ip(100), ip(200))
    };
    assert!(base().try_build().is_ok());
    assert_eq!(
        base().ip_pool(ip(200), ip(100)).try_build().unwrap_err(),
        ConfigError::PoolStartAfterEnd
    );
    assert_eq!(
        base()
            .ip_pool(ip(100), Ipv4Addr::new(10, 0, 1, 5))
            .try_build()
            .unwrap_err(),
        ConfigError::PoolOutsideSubnet
    );
    assert_eq!(
        base()
            .subnet_mask(Ipv4Addr::new(255, 0, 255, 0))
            .try_build()
            .unwrap_err(),
        ConfigError::InvalidSubnetMask
    );
    assert_eq!(
        base().lease_time(0).try_build().unwrap_err(),
        ConfigError::ZeroLeaseTime
    );
    // The builder's default router is on 192.168.1.0/24, not this subnet.
    assert_eq!(
        TestBuilder::new()
            .server_ip(ip(1))
            .ip_pool(ip(100), ip(200))
            .try_build()
            .unwrap_err(),
        ConfigError::RouterOutsideSubnet
    );
    assert!(base().router(ip(254)).try_build().is_ok());
    let long = core::str::from_utf8(&LONG).unwrap();
    assert_eq!(
        base().domain_name(long).try_build().unwrap_err(),
        ConfigError::DomainNameTooLong
    );
}

#[test]
fn server_new_uses_one_dns_server() {
    let server = TestServer::new(
        Ipv4Addr::new(192, 168, 1, 1),
        Ipv4Addr::new(255, 255, 255, 0),
        Ipv4Addr::new(192, 168, 1, 1),
        Ipv4Addr::new(8, 8, 4, 4),
        Ipv4Addr::new(192, 168, 1, 100),
        Ipv4Addr::new(192, 168, 1, 200),
    );
    assert_eq!(
        server.config().dns_servers.as_slice(),
        [Ipv4Addr::new(8, 8, 4, 4)]
    );
    assert_eq!(
        server.get_next_available_ip(),
        Some(Ipv4Addr::new(192, 168, 1, 100))
    );
}

// ---------------------------------------------------------------- relay pools

const RELAY: Ipv4Addr = Ipv4Addr::new(10, 0, 1, 1);

const fn remote(last: u8) -> Ipv4Addr {
    Ipv4Addr::new(10, 0, 1, last)
}

/// `server()` plus a pool on 10.0.1.0/24 behind the relay 10.0.1.1.
fn relayed_server() -> TestServer {
    let mut server = server();
    server
        .add_pool(
            SubnetPool::new(Ipv4Addr::new(255, 255, 255, 0), remote(100), remote(101))
                .with_router(RELAY),
        )
        .unwrap();
    server
}

/// DISCOVER and REQUEST through the relay.
fn relayed_lease(server: &mut TestServer, mac: [u8; 6]) -> Option<Ipv4Addr> {
    let (offer, _) = process(server, &Msg::new(DHCP_DISCOVER, mac).giaddr(RELAY), 0);
    let offered = offer?.yiaddr();
    let request = Msg::new(DHCP_REQUEST, mac)
        .giaddr(RELAY)
        .server_id(SERVER)
        .requested(offered);
    let (ack, _) = process(server, &request, 0);
    let ack = ack?;
    assert_eq!(ack.msg_type(), Some(DHCP_ACK));
    Some(ack.yiaddr())
}

#[test]
fn relayed_clients_get_addresses_from_their_pool() {
    let mut server = relayed_server();

    let (offer, _) = process(
        &mut server,
        &Msg::new(DHCP_DISCOVER, MAC_A).giaddr(RELAY),
        0,
    );
    let offer = offer.unwrap();
    assert_eq!(offer.yiaddr(), remote(100));
    assert_eq!(offer.to, (RELAY, DHCP_SERVER_PORT));
    // The pool's subnet configuration, the shared DNS server.
    assert_eq!(
        options::find_ipv4(offer.options(), options::ROUTER),
        Some(RELAY)
    );
    assert_eq!(
        options::find_ipv4(offer.options(), options::BROADCAST_ADDRESS),
        Some(remote(255))
    );
    assert_eq!(
        options::find_ipv4(offer.options(), options::SUBNET_MASK),
        Some(Ipv4Addr::new(255, 255, 255, 0))
    );
    assert_eq!(
        options::find_ipv4(offer.options(), options::SERVER_ID),
        Some(SERVER)
    );
    assert_eq!(offer.option(options::DNS_SERVER), Some(&[1u8, 1, 1, 1][..]));

    let request = Msg::new(DHCP_REQUEST, MAC_A)
        .giaddr(RELAY)
        .server_id(SERVER)
        .requested(remote(100));
    let (ack, event) = process(&mut server, &request, 0);
    assert_eq!(ack.unwrap().yiaddr(), remote(100));
    assert_eq!(event, Some(TransactionEvent::Leased(remote(100), MAC_A)));

    // Local clients still use the main pool.
    assert_eq!(lease(&mut server, MAC_B, 0), ip(100));
}

#[test]
fn each_pool_is_exhausted_on_its_own() {
    let mut server = relayed_server();
    assert_eq!(
        relayed_lease(&mut server, [2, 0, 0, 0, 3, 1]),
        Some(remote(100))
    );
    assert_eq!(
        relayed_lease(&mut server, [2, 0, 0, 0, 3, 2]),
        Some(remote(101))
    );
    // The remote pool is full, but the main pool is untouched.
    assert_eq!(relayed_lease(&mut server, [2, 0, 0, 0, 3, 3]), None);
    assert_eq!(server.get_next_available_ip(), Some(ip(100)));
}

#[test]
fn requests_from_unknown_relays_are_ignored() {
    let mut server = relayed_server();
    let stranger = Ipv4Addr::new(172, 16, 0, 1);
    for msg in [
        Msg::new(DHCP_DISCOVER, MAC_A).giaddr(stranger),
        Msg::new(DHCP_REQUEST, MAC_A)
            .giaddr(stranger)
            .requested(ip(100)),
        Msg::new(DHCP_INFORM, MAC_A)
            .giaddr(stranger)
            .ciaddr(Ipv4Addr::new(172, 16, 0, 9)),
    ] {
        assert!(process(&mut server, &msg, 0).0.is_none());
    }
    assert_eq!(server.lease_count(), 0);
}

#[test]
fn relay_on_the_servers_own_subnet_uses_the_main_pool() {
    let mut server = relayed_server();
    let local_relay = ip(254);
    let (offer, _) = process(
        &mut server,
        &Msg::new(DHCP_DISCOVER, MAC_A).giaddr(local_relay),
        0,
    );
    let offer = offer.unwrap();
    assert_eq!(offer.yiaddr(), ip(100));
    assert_eq!(offer.to, (local_relay, DHCP_SERVER_PORT));
}

#[test]
fn pool_infrastructure_addresses_are_skipped() {
    let mut server = server();
    // The whole /29 10.0.1.0-7, with the relay as router at .1.
    server
        .add_pool(
            SubnetPool::new(Ipv4Addr::new(255, 255, 255, 248), remote(0), remote(7))
                .with_router(RELAY),
        )
        .unwrap();
    let mut offered = heapless::Vec::<Ipv4Addr, 8>::new();
    for n in 0..4u8 {
        if let Some(address) = relayed_lease(&mut server, [2, 0, 0, 0, 4, n]) {
            offered.push(address).unwrap();
        }
    }
    // .0 network, .1 router, .7 broadcast are skipped; 4 leases fill the table.
    assert_eq!(
        offered.as_slice(),
        [remote(2), remote(3), remote(4), remote(5)]
    );
}

#[test]
fn client_moving_to_a_relayed_subnet_gets_a_new_address() {
    let mut server = relayed_server();
    assert_eq!(lease(&mut server, MAC_A, 0), ip(100));

    // Rebooting on the other subnet with the old address: refused.
    let reboot = Msg::new(DHCP_REQUEST, MAC_A)
        .giaddr(RELAY)
        .requested(ip(100));
    let (nak, _) = process(&mut server, &reboot, 1);
    assert_eq!(nak.unwrap().msg_type(), Some(DHCP_NAK));

    // Starting over there works, and replaces the old lease.
    assert_eq!(relayed_lease(&mut server, MAC_A), Some(remote(100)));
    assert_eq!(server.lease_for(MAC_A).unwrap().ip, remote(100));
    assert_eq!(server.get_next_available_ip(), Some(ip(100)));
}

#[test]
fn init_reboot_on_a_relayed_subnet_after_restart() {
    let mut server = relayed_server();
    let reboot = Msg::new(DHCP_REQUEST, MAC_A)
        .giaddr(RELAY)
        .requested(remote(101));
    let (ack, _) = process(&mut server, &reboot, 0);
    let ack = ack.unwrap();
    assert_eq!(ack.msg_type(), Some(DHCP_ACK));
    assert_eq!(ack.yiaddr(), remote(101));
}

#[test]
fn static_leases_on_relayed_subnets() {
    let mut server = server();
    // Before the pool exists, the address belongs to no known subnet.
    assert_eq!(
        server.add_static_lease(MAC_A, remote(50)),
        Err(StaticLeaseError::OutsideSubnet)
    );
    server
        .add_pool(SubnetPool::new(
            Ipv4Addr::new(255, 255, 255, 0),
            remote(100),
            remote(101),
        ))
        .unwrap();
    server.add_static_lease(MAC_A, remote(50)).unwrap();
    assert_eq!(
        server.add_static_lease(MAC_B, remote(255)),
        Err(StaticLeaseError::ReservedAddress)
    );

    // Through the relay the static address is used ...
    assert_eq!(relayed_lease(&mut server, MAC_A), Some(remote(50)));
    // ... and locally the client gets a dynamic address instead.
    let mut local = self::server();
    local
        .add_pool(SubnetPool::new(
            Ipv4Addr::new(255, 255, 255, 0),
            remote(100),
            remote(101),
        ))
        .unwrap();
    local.add_static_lease(MAC_A, remote(50)).unwrap();
    assert_eq!(lease(&mut local, MAC_A, 0), ip(100));
}

#[test]
fn add_pool_validation() {
    let mask = Ipv4Addr::new(255, 255, 255, 0);
    let mut server = DhcpServer::<4, 2, 2, 1>::with_config(
        TestBuilder::new()
            .server_ip(SERVER)
            .subnet_mask(mask)
            .ip_pool(ip(100), ip(103))
            .build(),
    );
    for (pool, error) in [
        (
            SubnetPool::new(Ipv4Addr::new(255, 0, 255, 0), remote(1), remote(9)),
            ConfigError::InvalidSubnetMask,
        ),
        (
            SubnetPool::new(mask, remote(9), remote(1)),
            ConfigError::PoolStartAfterEnd,
        ),
        (
            SubnetPool::new(mask, remote(1), Ipv4Addr::new(10, 0, 2, 9)),
            ConfigError::PoolOutsideSubnet,
        ),
        (
            SubnetPool::new(mask, remote(1), remote(9)).with_router(ip(1)),
            ConfigError::RouterOutsideSubnet,
        ),
        (
            SubnetPool::new(mask, ip(150), ip(160)),
            ConfigError::OverlappingSubnet,
        ),
        // A /16 that contains the server's /24.
        (
            SubnetPool::new(Ipv4Addr::new(255, 255, 0, 0), remote(1), remote(9)),
            ConfigError::OverlappingSubnet,
        ),
    ] {
        assert_eq!(server.add_pool(pool), Err(error), "{pool:?}");
    }
    assert!(server.pools().is_empty());

    let first = SubnetPool::new(mask, remote(1), remote(9));
    server.add_pool(first).unwrap();
    assert_eq!(server.pools(), [first]);
    assert_eq!(
        server.add_pool(SubnetPool::new(mask, remote(20), remote(30))),
        Err(ConfigError::OverlappingSubnet)
    );
    assert_eq!(
        server.add_pool(SubnetPool::new(
            mask,
            Ipv4Addr::new(10, 0, 2, 1),
            Ipv4Addr::new(10, 0, 2, 9)
        )),
        Err(ConfigError::TooManyPools)
    );
}

// ---------------------------------------------------------------- host names

#[test]
fn hostname_is_kept_with_the_lease() {
    let mut server = server();
    process(
        &mut server,
        &Msg::new(DHCP_DISCOVER, MAC_A).hostname(b"printer"),
        0,
    );
    let reserved = server.lease_for(MAC_A).unwrap();
    assert_eq!(reserved.hostname.unwrap().as_str(), "printer");

    // The request does not repeat the name; it is kept.
    let request = Msg::new(DHCP_REQUEST, MAC_A)
        .server_id(SERVER)
        .requested(ip(100));
    let action = server.prepare_packet_with_filter(&request.bytes(), &mut |_, _| true, 1);
    let (event, lease) = action.event.unwrap();
    assert_eq!(event, TransactionEvent::Leased(ip(100), MAC_A));
    assert_eq!(lease.hostname.unwrap().as_str(), "printer");
    assert!(lease.bound);

    // A later request with a new name replaces it.
    let renew = Msg::new(DHCP_REQUEST, MAC_A)
        .ciaddr(ip(100))
        .hostname(b"printer-2");
    process(&mut server, &renew, 2);
    assert_eq!(
        server.lease_for(MAC_A).unwrap().hostname.unwrap().as_str(),
        "printer-2"
    );

    // The release event carries the lease that was removed.
    let release = Msg::new(DHCP_RELEASE, MAC_A).ciaddr(ip(100));
    let action = server.prepare_packet_with_filter(&release.bytes(), &mut |_, _| true, 3);
    let (event, lease) = action.event.unwrap();
    assert_eq!(event, TransactionEvent::Released(ip(100), MAC_A));
    assert_eq!(lease.hostname.unwrap().as_str(), "printer-2");
}

#[test]
fn clients_without_a_hostname() {
    let mut server = server();
    lease(&mut server, MAC_A, 0);
    assert_eq!(server.lease_for(MAC_A).unwrap().hostname, None);
}

#[test]
fn hostname_parsing() {
    let parse = |bytes: &[u8]| {
        Hostname::parse(bytes).map(|name| heapless::String::<64>::try_from(name.as_str()).unwrap())
    };
    assert_eq!(parse(b"nas").as_deref(), Some("nas"));
    // NUL-terminated names, as some clients send them.
    assert_eq!(parse(b"nas\0\0").as_deref(), Some("nas"));
    assert_eq!(parse(b"").as_deref(), None);
    assert_eq!(parse(b"\0").as_deref(), None);
    // Invalid UTF-8 ends the name.
    assert_eq!(parse(b"cam\xff1").as_deref(), Some("cam"));
    assert_eq!(parse(b"\xffcam").as_deref(), None);

    // Long names are cut to MAX_HOSTNAME_LEN bytes, at a character boundary.
    let long = [b'a'; 40];
    assert_eq!(parse(&long).unwrap().len(), MAX_HOSTNAME_LEN);
    let mut accented = [b'a'; MAX_HOSTNAME_LEN + 1];
    // 'é' (2 bytes) straddling the limit is dropped whole.
    accented[MAX_HOSTNAME_LEN - 1] = 0xC3;
    accented[MAX_HOSTNAME_LEN] = 0xA9;
    assert_eq!(parse(&accented).unwrap().len(), MAX_HOSTNAME_LEN - 1);

    let name = Hostname::parse(b"nas").unwrap();
    let mut shown = heapless::String::<16>::new();
    core::fmt::write(&mut shown, format_args!("{name} {name:?}")).unwrap();
    assert_eq!(shown.as_str(), "nas \"nas\"");
}
