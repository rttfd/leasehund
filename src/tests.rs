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
        .domain_name("home.arpa")
        .build();
    assert_eq!(config.server_ip, Ipv4Addr::new(10, 0, 0, 1));
    assert_eq!(config.subnet_mask, Ipv4Addr::new(255, 255, 255, 0));
    assert_eq!(config.router, Some(Ipv4Addr::new(10, 0, 0, 254)));
    assert_eq!(config.dns_servers.len(), 1);
    assert_eq!(config.dns_servers[0], Ipv4Addr::new(8, 8, 8, 8));
    assert_eq!(config.ip_pool_start, Ipv4Addr::new(10, 0, 0, 100));
    assert_eq!(config.ip_pool_end, Ipv4Addr::new(10, 0, 0, 200));
    assert_eq!(config.lease_time, 3600);
    assert_eq!(config.domain_name, Some("home.arpa"));
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
    data.extend_from_slice(&[
        super::options::MESSAGE_TYPE,
        1,
        msg_type,
        super::options::END,
    ])
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

#[test]
fn pad_before_message_type_is_understood() {
    let mut server = test_server();
    let mac = [0, 1, 2, 3, 4, 5];
    let mut data = dhcp_packet(super::DHCP_DISCOVER, mac);
    // Insert two PAD options in front of the message type.
    data.insert(super::FIXED_PART_SIZE, super::options::PAD)
        .unwrap();
    data.insert(super::FIXED_PART_SIZE, super::options::PAD)
        .unwrap();

    let action = server.prepare_packet_with_filter(&data, &mut |_, _| true);

    assert!(action.is_some());
    assert_eq!(server.leases.len(), 1);
}

#[test]
fn config_validation() {
    use super::ConfigError;
    static LONG: [u8; 256] = [b'x'; 256];
    let ip = |last| Ipv4Addr::new(10, 0, 0, last);
    let base = || {
        TestBuilder::new()
            .server_ip(ip(1))
            .subnet_mask(Ipv4Addr::new(255, 255, 255, 0))
            .no_router()
            .ip_pool(ip(100), ip(200))
    };
    assert!(base().try_build().is_ok());
    assert!(TestConfig::default().validate().is_ok());
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
fn offer_carries_the_domain_name() {
    let config = TestBuilder::new()
        .server_ip(Ipv4Addr::new(10, 0, 0, 1))
        .router(Ipv4Addr::new(10, 0, 0, 1))
        .ip_pool(Ipv4Addr::new(10, 0, 0, 100), Ipv4Addr::new(10, 0, 0, 101))
        .domain_name("lan")
        .build();
    let mut server = TestServer::with_config(config);
    let data = dhcp_packet(super::DHCP_DISCOVER, [0, 1, 2, 3, 4, 5]);

    let action = server
        .prepare_packet_with_filter(&data, &mut |_, _| true)
        .unwrap();

    let reply = action.response.unwrap();
    let options = &reply[super::FIXED_PART_SIZE..];
    assert_eq!(
        super::options::find(options, super::options::DOMAIN_NAME),
        Some(&b"lan"[..])
    );
}
