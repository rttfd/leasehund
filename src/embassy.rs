//! The Embassy adapter: a UDP socket on port 67 and the loops that feed it to
//! the core. Compiled with the `embassy` feature (on by default).

use core::convert::Infallible;
use embassy_net::Stack;
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_time::{Duration, Instant, Timer};
use leasehund::event::{AdmissionEvent, TransactionEvent};
use leasehund::lease::Lease;
use leasehund::packet::DHCP_SERVER_PORT;
use leasehund::server::DhcpServer;

/// Reexported types from Embassy for convenience
pub use embassy_net::udp::{BindError, RecvError};

/// Socket buffer size, larger than any DHCP datagram we accept.
const SOCKET_BUFFER_SIZE: usize = 1024;

/// Pre-allocated UDP socket buffers and metadata for the DHCP server.
pub struct DHCPServerBuffers {
    rx_buffer: [u8; SOCKET_BUFFER_SIZE],
    tx_buffer: [u8; SOCKET_BUFFER_SIZE],
    rx_meta: [PacketMetadata; 16],
    tx_meta: [PacketMetadata; 16],
}

impl DHCPServerBuffers {
    /// Creates a new set of DHCP server buffers.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use leasehund::DHCPServerBuffers;
    /// let buffers = DHCPServerBuffers::new();
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self {
            rx_buffer: [0; SOCKET_BUFFER_SIZE],
            tx_buffer: [0; SOCKET_BUFFER_SIZE],
            rx_meta: [PacketMetadata::EMPTY; 16],
            tx_meta: [PacketMetadata::EMPTY; 16],
        }
    }
}

impl Default for DHCPServerBuffers {
    fn default() -> Self {
        Self::new()
    }
}

/// Wrapper around the Embassy UDP socket for DHCP server use.
pub struct DHCPServerSocket<'a> {
    socket: UdpSocket<'a>,
}

impl<'a> DHCPServerSocket<'a> {
    /// Creates a new DHCP server UDP socket bound to port 67.
    ///
    /// # Errors
    ///
    /// Returns [`BindError`] if the socket cannot be bound to port 67.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use leasehund::{DHCPServerSocket, DHCPServerBuffers};
    /// # use embassy_net::Stack;
    /// # fn example(stack: Stack<'static>) -> Result<(), leasehund::BindError> {
    /// let mut buffers = DHCPServerBuffers::new();
    /// let socket = DHCPServerSocket::new(stack, &mut buffers)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(stack: Stack<'a>, buffers: &'a mut DHCPServerBuffers) -> Result<Self, BindError> {
        let mut socket = UdpSocket::new(
            stack,
            &mut buffers.rx_meta,
            &mut buffers.rx_buffer,
            &mut buffers.tx_meta,
            &mut buffers.tx_buffer,
        );
        socket.bind(DHCP_SERVER_PORT)?;
        Ok(Self { socket })
    }
}

impl<
    const MAX_CLIENTS: usize,
    const MAX_DNS: usize,
    const MAX_STATIC: usize,
    const MAX_POOLS: usize,
> DhcpServer<MAX_CLIENTS, MAX_DNS, MAX_STATIC, MAX_POOLS>
{
    /// Removes all expired leases from the lease table.
    pub fn purge_expired_leases(&mut self) {
        self.purge_expired_leases_at(Instant::now().as_millis());
    }

    /// Handles an incoming DHCP packet.
    #[expect(
        clippy::future_not_send,
        reason = "Embassy sockets are not `Send`; embedded executors run tasks on one thread"
    )]
    async fn handle_packet_with_filter<P>(
        &mut self,
        socket: &DHCPServerSocket<'_>,
        data: &[u8],
        allow_client: &mut P,
    ) -> Option<(TransactionEvent, Lease)>
    where
        P: FnMut([u8; 6], AdmissionEvent) -> bool,
    {
        let outcome = self.handle_packet(data, Instant::now().as_millis(), &mut *allow_client);
        if let Some(reply) = &outcome.reply {
            let to = reply.destination();
            let _ = socket
                .socket
                .send_to(reply.bytes(), (*to.ip(), to.port()))
                .await;
        }
        outcome.event
    }

    /// Processes packets until a single lease or release transaction completes.
    ///
    /// # Errors
    ///
    /// Returns [`RecvError`] if there was an error receiving a packet.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use leasehund::{DhcpServer, DHCPServerBuffers, DHCPServerSocket, TransactionEvent};
    /// # use embassy_net::Stack;
    /// use core::net::Ipv4Addr;
    ///
    /// # async fn example(stack: Stack<'static>) -> Result<(), leasehund::BindError> {
    /// let mut server = DhcpServer::<32, 4>::new(
    ///     Ipv4Addr::new(192, 168, 1, 1),
    ///     Ipv4Addr::new(255, 255, 255, 0),
    ///     Ipv4Addr::new(192, 168, 1, 1),
    ///     Ipv4Addr::new(8, 8, 8, 8),
    ///     Ipv4Addr::new(192, 168, 1, 100),
    ///     Ipv4Addr::new(192, 168, 1, 200),
    /// );
    /// let mut buffers = DHCPServerBuffers::new();
    /// let mut socket = DHCPServerSocket::new(stack, &mut buffers)?;
    /// let _event: Result<TransactionEvent, _> = server.lease_one(&mut socket).await;
    /// # Ok(())
    /// # }
    /// ```
    #[expect(
        clippy::future_not_send,
        reason = "Embassy sockets are not `Send`; embedded executors run tasks on one thread"
    )]
    pub async fn lease_one(
        &mut self,
        socket: &mut DHCPServerSocket<'_>,
    ) -> Result<TransactionEvent, RecvError> {
        self.lease_one_with_filter(socket, |_, _| true).await
    }

    /// Processes packets until a single lease or release transaction completes, while applying
    /// a caller-provided admission policy before OFFER/ACK responses are sent.
    ///
    /// Returning `false` for a discover or request silently drops that packet without sending an
    /// OFFER, ACK, or NAK. Releases are always processed even if the policy returns `false`.
    ///
    /// # Errors
    ///
    /// Returns [`RecvError`] if there was an error receiving a packet.
    #[expect(
        clippy::future_not_send,
        reason = "Embassy sockets are not `Send`; embedded executors run tasks on one thread"
    )]
    pub async fn lease_one_with_filter<P>(
        &mut self,
        socket: &mut DHCPServerSocket<'_>,
        allow_client: P,
    ) -> Result<TransactionEvent, RecvError>
    where
        P: FnMut([u8; 6], AdmissionEvent) -> bool,
    {
        self.lease_one_with_lease(socket, allow_client)
            .await
            .map(|(event, _)| event)
    }

    /// Like [`lease_one_with_filter`](Self::lease_one_with_filter), also
    /// returning the [`Lease`] the transaction concerns: the new lease for
    /// [`TransactionEvent::Leased`], the removed one for
    /// [`TransactionEvent::Released`]. It carries details such as the client's
    /// host name.
    ///
    /// # Errors
    ///
    /// Returns [`RecvError`] if there was an error receiving a packet.
    #[expect(
        clippy::future_not_send,
        reason = "Embassy sockets are not `Send`; embedded executors run tasks on one thread"
    )]
    pub async fn lease_one_with_lease<P>(
        &mut self,
        socket: &mut DHCPServerSocket<'_>,
        mut allow_client: P,
    ) -> Result<(TransactionEvent, Lease), RecvError>
    where
        P: FnMut([u8; 6], AdmissionEvent) -> bool,
    {
        loop {
            // As large as the socket buffer, so no datagram is ever truncated.
            let mut buf = [0u8; SOCKET_BUFFER_SIZE];
            let (len, _) = socket.socket.recv_from(&mut buf).await?;
            let event = self
                .handle_packet_with_filter(socket, &buf[..len], &mut allow_client)
                .await;
            socket.socket.flush().await;
            if let Some(event) = event {
                return Ok(event);
            }
        }
    }

    /// Runs the DHCP server forever on the provided network stack.
    ///
    /// It never returns normally: the `Ok` type is uninhabited, so
    /// `let Err(error) = server.run(stack).await;` is irrefutable.
    ///
    /// # Errors
    ///
    /// Returns [`BindError`] if the UDP socket cannot be bound to port 67.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use embassy_net::Stack;
    /// use leasehund::DhcpServer;
    /// use core::net::Ipv4Addr;
    ///
    /// # async fn example(stack: Stack<'static>) -> Result<core::convert::Infallible, leasehund::BindError> {
    /// let mut server = DhcpServer::<32, 4>::new(
    ///     Ipv4Addr::new(192, 168, 1, 1),
    ///     Ipv4Addr::new(255, 255, 255, 0),
    ///     Ipv4Addr::new(192, 168, 1, 1),
    ///     Ipv4Addr::new(8, 8, 8, 8),
    ///     Ipv4Addr::new(192, 168, 1, 100),
    ///     Ipv4Addr::new(192, 168, 1, 200),
    /// );
    ///
    /// server.run(stack).await
    /// # }
    /// ```
    #[expect(
        clippy::future_not_send,
        reason = "Embassy sockets are not `Send`; embedded executors run tasks on one thread"
    )]
    pub async fn run(&mut self, stack: Stack<'_>) -> Result<Infallible, BindError> {
        self.run_with_filter_and_callback(stack, |_, _| true, |_| {})
            .await
    }

    /// Runs the DHCP server forever, invoking `callback` for every lease event.
    ///
    /// This is identical to [`run`](Self::run) except that `TransactionEvent`s
    /// are forwarded to the caller via `callback` instead of being discarded.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use embassy_net::Stack;
    /// use leasehund::{DhcpServer, TransactionEvent};
    /// use core::net::Ipv4Addr;
    ///
    /// # async fn example(stack: Stack<'static>) -> Result<core::convert::Infallible, leasehund::BindError> {
    /// let mut server = DhcpServer::<32, 4>::new(
    ///     Ipv4Addr::new(192, 168, 1, 1),
    ///     Ipv4Addr::new(255, 255, 255, 0),
    ///     Ipv4Addr::new(192, 168, 1, 1),
    ///     Ipv4Addr::new(8, 8, 8, 8),
    ///     Ipv4Addr::new(192, 168, 1, 100),
    ///     Ipv4Addr::new(192, 168, 1, 200),
    /// );
    ///
    /// server.run_with_callback(stack, |event| {
    ///     match event {
    ///         TransactionEvent::Leased(ip, mac) => {
    ///             // log or react to new lease
    ///         }
    ///         TransactionEvent::Released(ip, mac) => {
    ///             // log or react to release
    ///         }
    ///     }
    /// }).await
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`BindError`] if the UDP socket cannot be bound to port 67;
    /// otherwise it never returns.
    #[expect(
        clippy::future_not_send,
        reason = "Embassy sockets are not `Send`; embedded executors run tasks on one thread"
    )]
    pub async fn run_with_callback<F>(
        &mut self,
        stack: Stack<'_>,
        callback: F,
    ) -> Result<Infallible, BindError>
    where
        F: FnMut(TransactionEvent),
    {
        self.run_with_filter_and_callback(stack, |_, _| true, callback)
            .await
    }

    /// Runs the DHCP server forever, applying a caller-provided admission policy before OFFER/ACK
    /// responses are sent and invoking `callback` for every lease event that completes.
    ///
    /// Returning `false` for a discover or request silently drops that packet without sending an
    /// OFFER, ACK, or NAK. Releases are always processed even if the policy returns `false`.
    ///
    /// # Errors
    ///
    /// Returns [`BindError`] if the UDP socket cannot be bound to port 67;
    /// otherwise it never returns.
    #[expect(
        clippy::future_not_send,
        reason = "Embassy sockets are not `Send`; embedded executors run tasks on one thread"
    )]
    pub async fn run_with_filter_and_callback<P, F>(
        &mut self,
        stack: Stack<'_>,
        allow_client: P,
        mut callback: F,
    ) -> Result<Infallible, BindError>
    where
        P: FnMut([u8; 6], AdmissionEvent) -> bool,
        F: FnMut(TransactionEvent),
    {
        self.run_with_lease_callback(stack, allow_client, |event, _| callback(event))
            .await
    }

    /// Like [`run_with_filter_and_callback`](Self::run_with_filter_and_callback),
    /// also passing the [`Lease`] each event concerns: the new lease for
    /// [`TransactionEvent::Leased`], the removed one for
    /// [`TransactionEvent::Released`].
    ///
    /// ```rust,no_run
    /// use core::net::Ipv4Addr;
    /// use leasehund::{DhcpServer, TransactionEvent};
    /// # use embassy_net::Stack;
    ///
    /// # async fn example(
    /// #     stack: Stack<'static>,
    /// #     mut server: DhcpServer,
    /// # ) -> Result<core::convert::Infallible, leasehund::BindError> {
    /// server
    ///     .run_with_lease_callback(stack, |_, _| true, |event, lease| {
    ///         if let (TransactionEvent::Leased(ip, _), Some(name)) = (event, lease.hostname) {
    ///             // for example: show "printer.lan -> 192.168.1.50" on a display
    ///             let _ = (ip, name.as_str());
    ///         }
    ///     })
    ///     .await
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`BindError`] if the UDP socket cannot be bound to port 67;
    /// otherwise it never returns.
    #[expect(
        clippy::future_not_send,
        reason = "Embassy sockets are not `Send`; embedded executors run tasks on one thread"
    )]
    pub async fn run_with_lease_callback<P, F>(
        &mut self,
        stack: Stack<'_>,
        mut allow_client: P,
        mut callback: F,
    ) -> Result<Infallible, BindError>
    where
        P: FnMut([u8; 6], AdmissionEvent) -> bool,
        F: FnMut(TransactionEvent, &Lease),
    {
        let mut buffers = DHCPServerBuffers::new();
        let mut socket = DHCPServerSocket::new(stack, &mut buffers)?;
        loop {
            let mut buf = [0u8; SOCKET_BUFFER_SIZE];
            match socket.socket.recv_from(&mut buf).await {
                Ok((len, _)) => {
                    if let Some((event, lease)) = self
                        .handle_packet_with_filter(&socket, &buf[..len], &mut allow_client)
                        .await
                    {
                        callback(event, &lease);
                    }
                    socket.socket.flush().await;
                }
                Err(_) => Timer::after(Duration::from_millis(100)).await,
            }
        }
    }
}
