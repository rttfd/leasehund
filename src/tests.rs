use core::net::Ipv4Addr;

type TestServer = super::DhcpServer<2, 2>;
type TestConfig = super::DhcpConfig<2>;
type TestBuilder = super::DhcpConfigBuilder<2>;

#[test]
fn config_builder_basic() {
    let config = TestBuilder::new()
        .server_ip(Ipv4Addr::new(10, 0, 0, 1))
        .subnet_mask(Ipv4Addr::new(255, 255, 255, 0))
        .router(Ipv4Addr::new(10, 0, 0, 254))
        .add_dns_server(Ipv4Addr::new(8, 8, 8, 8))
        .ip_pool(Ipv4Addr::new(10, 0, 0, 100), Ipv4Addr::new(10, 0, 0, 200))
        .lease_time(3600)
        .build();
    assert_eq!(config.server_ip, Ipv4Addr::new(10, 0, 0, 1));
    assert_eq!(config.subnet_mask, Ipv4Addr::new(255, 255, 255, 0));
    assert_eq!(config.router, Some(Ipv4Addr::new(10, 0, 0, 254)));
    assert_eq!(config.dns_servers.len(), 1);
    assert_eq!(config.dns_servers[0], Ipv4Addr::new(8, 8, 8, 8));
    assert_eq!(config.ip_pool_start, Ipv4Addr::new(10, 0, 0, 100));
    assert_eq!(config.ip_pool_end, Ipv4Addr::new(10, 0, 0, 200));
    assert_eq!(config.lease_time, 3600);
}

#[test]
fn config_builder_starts_with_no_dns() {
    let config = TestBuilder::new().build();
    assert!(config.dns_servers.is_empty());
}

#[test]
fn config_builder_no_router() {
    let config = TestBuilder::new().no_router().build();
    assert_eq!(config.router, None);
}

#[test]
fn server_new() {
    let server = TestServer::new(
        Ipv4Addr::new(192, 168, 1, 1),
        Ipv4Addr::new(255, 255, 255, 0),
        Ipv4Addr::new(192, 168, 1, 1),
        Ipv4Addr::new(8, 8, 4, 4),
        Ipv4Addr::new(192, 168, 1, 100),
        Ipv4Addr::new(192, 168, 1, 200),
    );
    let config = server.config();
    assert_eq!(config.server_ip, Ipv4Addr::new(192, 168, 1, 1));
    assert_eq!(config.dns_servers.len(), 1);
    assert_eq!(config.dns_servers[0], Ipv4Addr::new(8, 8, 4, 4));
}

#[test]
fn ip_pool_full() {
    let mut server = TestServer::new(
        Ipv4Addr::new(10, 0, 0, 1),
        Ipv4Addr::new(255, 255, 255, 0),
        Ipv4Addr::new(10, 0, 0, 1),
        Ipv4Addr::new(1, 1, 1, 1),
        Ipv4Addr::new(10, 0, 0, 100),
        Ipv4Addr::new(10, 0, 0, 101),
    );
    for i in 0..2 {
        let mac = [0, 0, 0, 0, 0, i];
        let lease = super::LeaseEntry {
            ip: Ipv4Addr::new(10, 0, 0, 100 + i),
            mac,
            expires_at: u64::MAX,
        };
        let _ = server.leases.insert(mac, lease);
    }
    assert!(server.is_pool_full());
}

#[test]
fn config_default_values() {
    let config = TestConfig::default();
    assert_eq!(config.server_ip, Ipv4Addr::new(192, 168, 1, 1));
    assert_eq!(config.subnet_mask, Ipv4Addr::new(255, 255, 255, 0));
    assert_eq!(config.router, Some(Ipv4Addr::new(192, 168, 1, 1)));
    assert_eq!(config.dns_servers.len(), 1);
    assert_eq!(config.dns_servers[0], Ipv4Addr::new(8, 8, 8, 8));
    assert_eq!(config.ip_pool_start, Ipv4Addr::new(192, 168, 1, 100));
    assert_eq!(config.ip_pool_end, Ipv4Addr::new(192, 168, 1, 200));
    assert_eq!(config.lease_time, super::DEFAULT_LEASE_TIME);
}

#[test]
fn get_next_available_ip_empty() {
    let server = TestServer::new(
        Ipv4Addr::new(10, 0, 0, 1),
        Ipv4Addr::new(255, 255, 255, 0),
        Ipv4Addr::new(10, 0, 0, 1),
        Ipv4Addr::new(1, 1, 1, 1),
        Ipv4Addr::new(10, 0, 0, 100),
        Ipv4Addr::new(10, 0, 0, 101),
    );
    assert_eq!(
        server.get_next_available_ip(),
        Some(Ipv4Addr::new(10, 0, 0, 100))
    );
}

#[test]
fn get_next_available_ip_skips_leased() {
    let mut server = TestServer::new(
        Ipv4Addr::new(10, 0, 0, 1),
        Ipv4Addr::new(255, 255, 255, 0),
        Ipv4Addr::new(10, 0, 0, 1),
        Ipv4Addr::new(1, 1, 1, 1),
        Ipv4Addr::new(10, 0, 0, 100),
        Ipv4Addr::new(10, 0, 0, 101),
    );
    let mac = [0, 0, 0, 0, 0, 1];
    let lease = super::LeaseEntry {
        ip: Ipv4Addr::new(10, 0, 0, 100),
        mac,
        expires_at: u64::MAX,
    };
    let _ = server.leases.insert(mac, lease);
    assert_eq!(
        server.get_next_available_ip(),
        Some(Ipv4Addr::new(10, 0, 0, 101))
    );
}

#[test]
fn parse_message_type_discover() {
    let options = [
        super::OPTION_MESSAGE_TYPE,
        1,
        super::DHCP_DISCOVER,
        super::OPTION_END,
    ];
    assert_eq!(
        TestServer::parse_message_type(&options),
        Some(super::DHCP_DISCOVER)
    );
}

#[test]
fn parse_message_type_empty() {
    assert_eq!(TestServer::parse_message_type(&[]), None);
}

#[test]
fn parse_message_type_end_only() {
    assert_eq!(TestServer::parse_message_type(&[super::OPTION_END]), None);
}

fn test_server() -> TestServer {
    TestServer::new(
        Ipv4Addr::new(10, 0, 0, 1),
        Ipv4Addr::new(255, 255, 255, 0),
        Ipv4Addr::new(10, 0, 0, 1),
        Ipv4Addr::new(1, 1, 1, 1),
        Ipv4Addr::new(10, 0, 0, 100),
        Ipv4Addr::new(10, 0, 0, 101),
    )
}

fn dhcp_packet(msg_type: u8, mac: [u8; 6]) -> heapless::Vec<u8, { super::DHCP_PACKET_SIZE }> {
    let mut packet = super::DhcpPacket::default();
    packet.chaddr[..6].copy_from_slice(&mac);
    let fixed: [u8; super::FIXED_PART_SIZE] = unsafe { core::mem::transmute(packet) };
    let mut data = heapless::Vec::new();
    data.extend_from_slice(&fixed).unwrap();
    data.extend_from_slice(&[super::OPTION_MESSAGE_TYPE, 1, msg_type, super::OPTION_END])
        .unwrap();
    data
}

#[test]
fn denied_discover_produces_no_response_or_lease() {
    let mut server = test_server();
    let mac = [0, 1, 2, 3, 4, 5];
    let data = dhcp_packet(super::DHCP_DISCOVER, mac);
    let mut deny = |received_mac, event| {
        assert_eq!(received_mac, mac);
        assert_eq!(event, super::AdmissionEvent::Discover);
        false
    };

    let action = server.prepare_packet_with_filter(&data, &mut deny);

    assert!(action.is_none());
    assert!(server.leases.is_empty());
}

#[test]
fn denied_request_produces_no_response_or_lease() {
    let mut server = test_server();
    let mac = [0, 1, 2, 3, 4, 5];
    let data = dhcp_packet(super::DHCP_REQUEST, mac);
    let mut deny = |received_mac, event| {
        assert_eq!(received_mac, mac);
        assert_eq!(event, super::AdmissionEvent::Request);
        false
    };

    let action = server.prepare_packet_with_filter(&data, &mut deny);

    assert!(action.is_none());
    assert!(server.leases.is_empty());
}
