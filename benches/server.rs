//! Benchmarks for the per-packet hot path: the transport-free core turning a
//! client's datagram into a reply.

#![expect(
    clippy::expect_used,
    reason = "a benchmark with broken setup should stop loudly, like a test"
)]

use core::net::Ipv4Addr;
use criterion::{Criterion, criterion_group, criterion_main};
use leasehund::{DhcpConfigBuilder, DhcpServer};
use std::hint::black_box;

const DISCOVER: u8 = 1;
const REQUEST: u8 = 3;
const SERVER: Ipv4Addr = Ipv4Addr::new(10, 0, 0, 1);

/// A BOOTREQUEST from `mac` carrying the given options (type first).
fn client_message(msg_type: u8, mac: [u8; 6], extra: &[u8]) -> Vec<u8> {
    let mut packet = vec![0u8; 240];
    packet[..4].copy_from_slice(&[1, 1, 6, 0]); // BOOTREQUEST, Ethernet, 6-byte address
    packet[4..8].copy_from_slice(&0x1234_5678u32.to_be_bytes());
    packet[28..34].copy_from_slice(&mac);
    packet[236..240].copy_from_slice(&[0x63, 0x82, 0x53, 0x63]);
    packet.extend_from_slice(&[53, 1, msg_type]);
    packet.extend_from_slice(extra);
    packet.push(255);
    packet
}

fn server() -> DhcpServer<32, 4> {
    let config = DhcpConfigBuilder::<4>::new()
        .server_ip(SERVER)
        .router(SERVER)
        .subnet_mask(Ipv4Addr::new(255, 255, 255, 0))
        .add_dns_server(Ipv4Addr::new(1, 1, 1, 1))
        .ip_pool(Ipv4Addr::new(10, 0, 0, 100), Ipv4Addr::new(10, 0, 0, 200))
        .domain_name("lan")
        .try_build()
        .expect("valid configuration");
    DhcpServer::with_config(config)
}

fn handle_packet(c: &mut Criterion) {
    let mac = [0x02, 0, 0, 0, 0, 1];
    let discover = client_message(
        DISCOVER,
        mac,
        &[12, 7, b'p', b'r', b'i', b'n', b't', b'e', b'r'],
    );
    let mut options = vec![54, 4];
    options.extend_from_slice(&SERVER.octets());
    options.extend_from_slice(&[50, 4, 10, 0, 0, 100]);
    let request = client_message(REQUEST, mac, &options);

    c.bench_function("handle_packet/discover", |b| {
        let mut server = server();
        b.iter(|| {
            let outcome = server.handle_packet(black_box(&discover), 0, |_, _| true);
            black_box(outcome.reply.expect("offer").bytes().len())
        });
    });

    c.bench_function("handle_packet/discover_request", |b| {
        let mut server = server();
        b.iter(|| {
            let offer = server.handle_packet(black_box(&discover), 0, |_, _| true);
            let ack = server.handle_packet(black_box(&request), 0, |_, _| true);
            black_box((offer.reply.is_some(), ack.event.is_some()))
        });
    });

    // The address search walks the pool, so a busy table is the slow case.
    c.bench_function("handle_packet/discover_with_31_leases", |b| {
        let mut server = server();
        for client in 0..31u8 {
            let other = [0x02, 0, 0, 0, 1, client];
            let offer = server
                .handle_packet(&client_message(DISCOVER, other, &[]), 0, |_, _| true)
                .reply
                .expect("offer");
            let ip = &offer.bytes()[16..20];
            let mut options = vec![54, 4];
            options.extend_from_slice(&SERVER.octets());
            options.extend_from_slice(&[50, 4]);
            options.extend_from_slice(ip);
            server.handle_packet(&client_message(REQUEST, other, &options), 0, |_, _| true);
        }
        assert_eq!(server.lease_count(), 31);
        b.iter(|| {
            let outcome = server.handle_packet(black_box(&discover), 0, |_, _| true);
            // Drop the reservation, so the next offer searches the pool again.
            server.revoke(mac);
            black_box(outcome.reply.expect("offer").bytes().len())
        });
    });
}

criterion_group!(benches, handle_packet);
criterion_main!(benches);
