use gbnet::{
    BitBuffer, BitDeserialize, BitSerialize, Channel, ChannelConfig, ClientEvent, Connection,
    FragmentAssembler, NetClient, NetServer, NetworkConfig, NetworkSimulator, Packet, PacketHeader,
    PacketType, ServerEvent, SimulationConfig, UdpSocket,
};

use gbnet::NetworkSerialize;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::thread;
use std::time::Duration;

#[derive(NetworkSerialize, Debug, PartialEq)]
struct GamePacket {
    #[bits = 16]
    player_id: u16,
    #[bits = 10]
    x: u16,
    #[bits = 10]
    y: u16,
    #[bits = 8]
    health: u8,
}

#[test]
fn test_full_packet_flow() -> std::io::Result<()> {
    let game_data = GamePacket {
        player_id: 12345,
        x: 512,
        y: 768,
        health: 100,
    };

    let mut buffer = BitBuffer::new();
    game_data.bit_serialize(&mut buffer)?;
    let payload = buffer.into_bytes(true)?;

    let header = PacketHeader {
        protocol_id: 0x12345678,
        sequence: 1,
        ack: 0,
        ack_bits: 0,
    };

    let packet = Packet::new(
        header,
        PacketType::Payload {
            channel: 0,
            is_fragment: false,
        },
    )
    .with_payload(payload);

    let packet_data = packet.serialize()?;

    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let client_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);

    let mut server = UdpSocket::bind(server_addr)
        .map_err(|e| std::io::Error::other(format!("Socket error: {:?}", e)))?;
    let mut client = UdpSocket::bind(client_addr)
        .map_err(|e| std::io::Error::other(format!("Socket error: {:?}", e)))?;

    let actual_server_addr = server
        .local_addr()
        .map_err(|e| std::io::Error::other(format!("Socket error: {:?}", e)))?;

    client
        .send_to(&packet_data, actual_server_addr)
        .map_err(|e| std::io::Error::other(format!("Socket error: {:?}", e)))?;

    thread::sleep(Duration::from_millis(10));
    let (received_data, _from) = server
        .recv_from()
        .map_err(|e| std::io::Error::other(format!("Socket error: {:?}", e)))?;

    let received_packet = Packet::deserialize(received_data)?;

    let mut payload_buffer = BitBuffer::from_bytes(received_packet.payload);
    let received_game_data = GamePacket::bit_deserialize(&mut payload_buffer)?;

    assert_eq!(received_game_data, game_data);
    assert_eq!(received_packet.header.sequence, 1);

    Ok(())
}

#[test]
fn test_connection_handshake_simulation() {
    let config = NetworkConfig::default();

    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let client_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);

    let _server_socket = UdpSocket::bind(server_addr).unwrap();
    let _client_socket = UdpSocket::bind(client_addr).unwrap();

    let actual_server_addr = _server_socket.local_addr().unwrap();
    let actual_client_addr = _client_socket.local_addr().unwrap();

    let mut client_conn = Connection::new(config.clone(), actual_server_addr, actual_client_addr);
    let server_conn = Connection::new(config, actual_server_addr, actual_client_addr);

    client_conn.connect().unwrap();

    assert!(!client_conn.is_connected());
    assert!(!server_conn.is_connected());
}

#[test]
fn test_multi_channel_messages() -> std::io::Result<()> {
    let reliable_config = ChannelConfig::reliable_ordered();
    let unreliable_config = ChannelConfig::unreliable();

    let mut reliable_sender = Channel::new(0, reliable_config);
    let mut reliable_receiver = Channel::new(0, reliable_config);
    let mut unreliable_sender = Channel::new(1, unreliable_config);
    let mut unreliable_receiver = Channel::new(1, unreliable_config);

    reliable_sender
        .send(b"important data", true)
        .map_err(|e| std::io::Error::other(format!("Channel error: {:?}", e)))?;
    unreliable_sender
        .send(b"position update", false)
        .map_err(|e| std::io::Error::other(format!("Channel error: {:?}", e)))?;

    let (_seq1, wire1) = reliable_sender.get_outgoing_message().unwrap();
    let (_seq2, wire2) = unreliable_sender.get_outgoing_message().unwrap();

    reliable_receiver.on_packet_received(wire1);
    unreliable_receiver.on_packet_received(wire2);

    assert_eq!(reliable_receiver.receive().unwrap(), b"important data");
    assert_eq!(unreliable_receiver.receive().unwrap(), b"position update");

    Ok(())
}

#[test]
fn test_client_server_handshake() {
    let config = NetworkConfig::default();
    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);

    let mut server = NetServer::bind(server_addr, config.clone()).unwrap();
    let actual_server_addr = server.local_addr().unwrap();

    let mut client = NetClient::connect(actual_server_addr, config).unwrap();

    // Run update loop to complete handshake
    for _ in 0..20 {
        let server_events = server.update();
        let client_events = client.update();

        for event in &client_events {
            if let ClientEvent::Connected = event {
                // Client connected
                assert!(client.is_connected());
            }
        }

        for event in &server_events {
            if let ServerEvent::ClientConnected(_addr) = event {
                assert_eq!(server.client_count(), 1);
            }
        }

        if client.is_connected() && server.client_count() == 1 {
            break;
        }

        thread::sleep(Duration::from_millis(10));
    }

    assert!(client.is_connected());
    assert_eq!(server.client_count(), 1);
}

#[test]
fn test_client_server_message_exchange() {
    let config = NetworkConfig::default();
    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);

    let mut server = NetServer::bind(server_addr, config.clone()).unwrap();
    let actual_server_addr = server.local_addr().unwrap();
    let mut client = NetClient::connect(actual_server_addr, config).unwrap();

    // Complete handshake
    for _ in 0..20 {
        server.update();
        client.update();
        if client.is_connected() && server.client_count() == 1 {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    assert!(client.is_connected());

    // Client sends message
    client.send(0, b"hello server").unwrap();
    client.update();

    thread::sleep(Duration::from_millis(10));

    let events = server.update();
    let mut got_message = false;
    for event in &events {
        if let ServerEvent::Message { data, .. } = event {
            assert_eq!(data, b"hello server");
            got_message = true;
        }
    }
    assert!(got_message, "Server should have received the message");
}

#[test]
fn test_server_max_connections() {
    let config = NetworkConfig {
        max_clients: 1,
        ..Default::default()
    };
    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);

    let mut server = NetServer::bind(server_addr, config.clone()).unwrap();
    let actual_server_addr = server.local_addr().unwrap();

    // Connect first client
    let mut client1 = NetClient::connect(actual_server_addr, config.clone()).unwrap();
    for _ in 0..20 {
        server.update();
        client1.update();
        if client1.is_connected() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(client1.is_connected());

    // Second client should be rejected
    let mut client2 = NetClient::connect(actual_server_addr, config).unwrap();
    for _ in 0..20 {
        server.update();
        let events = client2.update();
        for event in &events {
            if let ClientEvent::Disconnected(_) = event {
                return; // Expected
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
    // If we get here, the second client just times out (also acceptable)
    assert!(!client2.is_connected());
}

#[test]
fn test_graceful_disconnect() {
    let config = NetworkConfig::default();
    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);

    let mut server = NetServer::bind(server_addr, config.clone()).unwrap();
    let actual_server_addr = server.local_addr().unwrap();
    let mut client = NetClient::connect(actual_server_addr, config).unwrap();

    // Connect
    for _ in 0..20 {
        server.update();
        client.update();
        if client.is_connected() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(client.is_connected());

    // Client disconnects
    client.disconnect();
    thread::sleep(Duration::from_millis(10));

    let events = server.update();
    let mut disconnected = false;
    for event in &events {
        if let ServerEvent::ClientDisconnected(_, _) = event {
            disconnected = true;
        }
    }
    assert!(disconnected || server.client_count() == 0);
}

#[test]
fn test_server_broadcast() {
    let config = NetworkConfig::default();
    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);

    let mut server = NetServer::bind(server_addr, config.clone()).unwrap();
    let actual_server_addr = server.local_addr().unwrap();

    let mut client1 = NetClient::connect(actual_server_addr, config.clone()).unwrap();
    let mut client2 = NetClient::connect(actual_server_addr, config).unwrap();

    // Connect both
    for _ in 0..30 {
        server.update();
        client1.update();
        client2.update();
        if client1.is_connected() && client2.is_connected() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    if server.client_count() >= 2 {
        server.broadcast(0, b"broadcast msg", None);
        // Flush
        server.update();
        thread::sleep(Duration::from_millis(10));

        let e1 = client1.update();
        let e2 = client2.update();

        let got1 = e1.iter().any(|e| matches!(e, ClientEvent::Message { .. }));
        let got2 = e2.iter().any(|e| matches!(e, ClientEvent::Message { .. }));

        // At least one client should receive the broadcast
        assert!(
            got1 || got2,
            "At least one client should receive the broadcast message"
        );
    }
}

#[test]
fn test_fragmentation_end_to_end() {
    let data = vec![0xABu8; 5000]; // Larger than MTU
    let fragments = gbnet::fragment::fragment_message(1, &data, 1024).unwrap();
    assert!(fragments.len() > 1, "Should produce multiple fragments");

    let mut assembler = FragmentAssembler::new(Duration::from_secs(5), 1024 * 1024);

    // Deliver fragments out of order
    let mut result = None;
    let len = fragments.len();
    for i in (0..len).rev() {
        result = assembler.process_fragment(&fragments[i]);
    }

    let assembled = result.expect("Should reassemble all fragments");
    assert_eq!(assembled, data);
}

#[test]
fn test_batching_integration() {
    use gbnet::congestion::{batch_messages, unbatch_messages};

    let messages: Vec<Vec<u8>> = (0..20)
        .map(|i| format!("message {}", i).into_bytes())
        .collect();

    let batches = batch_messages(&messages, 200);
    assert!(batches.len() > 1, "Should produce multiple batches");

    let mut recovered = Vec::new();
    for batch in &batches {
        let msgs = unbatch_messages(batch).expect("Should unbatch");
        recovered.extend(msgs);
    }
    assert_eq!(recovered, messages);
}

#[test]
fn test_ordered_channel_timeout_recovery() {
    let mut config = ChannelConfig::reliable_ordered();
    config.ordered_buffer_timeout = Duration::from_millis(50);
    let mut ch = Channel::new(0, config);

    // Receive seq 1 and 2, skip seq 0
    let make_wire = |seq: u16, data: &[u8]| -> Vec<u8> {
        let mut wire = Vec::with_capacity(2 + data.len());
        wire.extend_from_slice(&seq.to_be_bytes());
        wire.extend_from_slice(data);
        wire
    };

    ch.on_packet_received(make_wire(1, b"msg1"));
    ch.on_packet_received(make_wire(2, b"msg2"));

    // Nothing delivered yet (waiting for seq 0)
    assert!(ch.receive().is_none());

    // Wait for timeout
    thread::sleep(Duration::from_millis(60));
    ch.update();

    // Should have delivered buffered messages after skipping the gap
    let msg1 = ch.receive().expect("Should deliver msg1 after timeout");
    assert_eq!(msg1, b"msg1");
    let msg2 = ch.receive().expect("Should deliver msg2 after timeout");
    assert_eq!(msg2, b"msg2");
}

#[test]
fn test_server_keepalive_prevents_timeout() {
    let config = NetworkConfig {
        connection_timeout: Duration::from_millis(500),
        keepalive_interval: Duration::from_millis(100),
        ..Default::default()
    };

    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let mut server = NetServer::bind(server_addr, config.clone()).unwrap();
    let actual_server_addr = server.local_addr().unwrap();
    let mut client = NetClient::connect(actual_server_addr, config).unwrap();

    // Connect
    for _ in 0..20 {
        server.update();
        client.update();
        if client.is_connected() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(client.is_connected());

    // Run idle for a while — keepalives should prevent timeout
    for _ in 0..10 {
        server.update();
        client.update();
        thread::sleep(Duration::from_millis(60));
    }

    // Both should still be connected
    assert!(client.is_connected());
    assert_eq!(server.client_count(), 1);
}

#[test]
fn test_network_simulator_loss_and_delivery() {
    let config = SimulationConfig {
        packet_loss: 0.0,
        latency_ms: 10,
        jitter_ms: 5,
        duplicate_chance: 0.0,
        out_of_order_chance: 0.0,
        bandwidth_limit_bytes_per_sec: 0,
    };
    let mut sim = NetworkSimulator::new(config);
    let addr: SocketAddr = "127.0.0.1:1234".parse().unwrap();

    // Send packets through simulator
    for i in 0u8..10 {
        sim.process_send(&[i], addr);
    }

    // Wait for delivery
    thread::sleep(Duration::from_millis(20));
    let ready = sim.receive_ready();
    assert_eq!(ready.len(), 10, "All packets should arrive with 0% loss");
}

#[test]
fn test_fragment_too_many_fragments_error() {
    // 25600 bytes / 100 = 256 fragments > MAX_FRAGMENT_COUNT (255)
    let big_data = vec![0u8; 256 * 100];
    let result = gbnet::fragment::fragment_message(0, &big_data, 100);
    assert!(result.is_err(), "Should error on too many fragments");
}
