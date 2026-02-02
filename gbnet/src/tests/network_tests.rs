use crate::{
    channel::{Channel, ChannelError},
    config::{
        ChannelConfig, ConfigError, DeliveryMode, NetworkConfig, DEFAULT_MTU, MAX_CHANNEL_COUNT,
        MAX_MTU, MIN_MTU,
    },
    connection::{Connection, ConnectionError},
    packet::{sequence_diff, sequence_greater_than, Packet, PacketHeader, PacketType},
    reliability::{ReliableEndpoint, SequenceBuffer},
    socket::UdpSocket,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

#[test]
fn test_socket_basic() {
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let socket = UdpSocket::bind(addr).unwrap();
    assert!(socket.local_addr().is_ok());
}

#[test]
fn test_packet_construction() {
    let header = PacketHeader {
        protocol_id: 0x12345678,
        sequence: 100,
        ack: 99,
        ack_bits: 0xFFFFFFFF,
    };

    let packet = Packet::new(header.clone(), PacketType::KeepAlive);
    assert_eq!(packet.header.protocol_id, 0x12345678);
    assert_eq!(packet.header.sequence, 100);
    assert!(packet.payload.is_empty());
}

#[test]
fn test_sequence_math() {
    assert!(sequence_greater_than(1, 0));
    assert!(!sequence_greater_than(0, 1));
    assert!(sequence_greater_than(0, 65535));
    assert!(!sequence_greater_than(65535, 0));

    assert_eq!(sequence_diff(5, 3), 2);
    assert_eq!(sequence_diff(3, 5), -2);
    assert_eq!(sequence_diff(0, 65535), 1);
}

#[test]
fn test_channel_send_receive() {
    let config = ChannelConfig::reliable_ordered();
    let mut sender = Channel::new(0, config);
    let mut receiver = Channel::new(0, config);

    sender.send(b"test message", true).unwrap();
    let (_seq, wire) = sender.get_outgoing_message().unwrap();

    receiver.on_packet_received(wire);
    let received = receiver.receive().unwrap();
    assert_eq!(received, b"test message");
}

#[test]
fn test_channel_buffer_full() {
    let mut config = ChannelConfig::reliable_ordered();
    config.message_buffer_size = 2;
    config.block_on_full = true;

    let mut channel = Channel::new(0, config);

    assert!(channel.send(b"msg1", false).is_ok());
    assert!(channel.send(b"msg2", false).is_ok());
    assert!(matches!(
        channel.send(b"msg3", false),
        Err(ChannelError::BufferFull)
    ));
}

#[test]
fn test_reliable_endpoint_sequences() {
    let mut endpoint = ReliableEndpoint::new(256);

    assert_eq!(endpoint.next_sequence(), 0);
    assert_eq!(endpoint.next_sequence(), 1);
    assert_eq!(endpoint.next_sequence(), 2);

    for i in 3..10 {
        assert_eq!(endpoint.next_sequence(), i);
    }
}

#[test]
fn test_reliable_endpoint_tracking() {
    let mut endpoint = ReliableEndpoint::new(256);
    let now = Instant::now();

    endpoint.on_packet_sent(0, now, vec![1, 2, 3]);
    endpoint.on_packet_sent(1, now, vec![4, 5, 6]);

    let stats = endpoint.stats();
    assert_eq!(stats.packets_in_flight, 2);

    endpoint.process_acks(0, 0);
    let stats = endpoint.stats();
    assert_eq!(stats.packets_in_flight, 1);
}

#[test]
fn test_sequence_buffer_operations() {
    let mut buffer: SequenceBuffer<u32> = SequenceBuffer::new(16);

    buffer.insert(0, 100);
    buffer.insert(1, 200);
    buffer.insert(2, 300);

    assert!(buffer.exists(0));
    assert!(buffer.exists(1));
    assert!(buffer.exists(2));
    assert!(!buffer.exists(3));

    assert_eq!(*buffer.get(0).unwrap(), 100);
    assert_eq!(*buffer.get(1).unwrap(), 200);
    assert_eq!(*buffer.get(2).unwrap(), 300);
}

#[test]
fn test_connection_states() {
    let config = NetworkConfig::default();
    let local = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let remote = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1234);

    let mut conn = Connection::new(config, local, remote);

    assert!(!conn.is_connected());
    assert_eq!(conn.local_addr(), local);
    assert_eq!(conn.remote_addr(), remote);

    assert!(conn.connect().is_ok());
    assert!(matches!(
        conn.connect(),
        Err(ConnectionError::AlreadyConnected)
    ));
}

#[test]
fn test_config_defaults() {
    let config = NetworkConfig::default();

    assert_eq!(config.protocol_id, 0x12345678);
    assert_eq!(config.max_clients, 64);
    assert_eq!(config.mtu, 1200);
    assert_eq!(config.max_channels, 8);

    let channel_config = config.default_channel_config;
    assert_eq!(channel_config.delivery_mode, DeliveryMode::ReliableOrdered);
}

#[test]
fn test_all_delivery_modes() {
    let modes = [
        ChannelConfig::unreliable(),
        ChannelConfig::unreliable_sequenced(),
        ChannelConfig::reliable_unordered(),
        ChannelConfig::reliable_ordered(),
        ChannelConfig::reliable_sequenced(),
    ];

    for config in &modes {
        let mut sender = Channel::new(0, *config);
        let mut receiver = Channel::new(0, *config);

        sender.send(b"data", true).unwrap();
        let (_seq, wire) = sender.get_outgoing_message().unwrap();
        receiver.on_packet_received(wire);

        let msg = receiver.receive();
        assert!(msg.is_some(), "Failed for mode {:?}", config.delivery_mode);
        assert_eq!(msg.unwrap(), b"data");
    }
}

// ─── Config validation tests ────────────────────────────────────────────────

#[test]
fn test_config_validation_default_passes() {
    let config = NetworkConfig::default();
    assert!(config.validate().is_ok());
}

#[test]
fn test_config_validation_fragment_threshold_exceeds_mtu() {
    let config = NetworkConfig {
        fragment_threshold: DEFAULT_MTU + 1,
        ..Default::default()
    };
    assert!(matches!(
        config.validate(),
        Err(ConfigError::FragmentThresholdExceedsMtu)
    ));
}

#[test]
fn test_config_validation_zero_channels() {
    let config = NetworkConfig {
        max_channels: 0,
        ..Default::default()
    };
    assert!(matches!(
        config.validate(),
        Err(ConfigError::InvalidChannelCount)
    ));
}

#[test]
fn test_config_validation_too_many_channels() {
    let config = NetworkConfig {
        max_channels: MAX_CHANNEL_COUNT + 1,
        ..Default::default()
    };
    assert!(matches!(
        config.validate(),
        Err(ConfigError::InvalidChannelCount)
    ));
}

#[test]
fn test_config_validation_zero_packet_buffer() {
    let config = NetworkConfig {
        packet_buffer_size: 0,
        ..Default::default()
    };
    assert!(matches!(
        config.validate(),
        Err(ConfigError::InvalidPacketBufferSize)
    ));
}

#[test]
fn test_config_validation_mtu_too_small() {
    let config = NetworkConfig {
        mtu: MIN_MTU - 1,
        fragment_threshold: MIN_MTU - 1,
        ..Default::default()
    };
    assert!(matches!(config.validate(), Err(ConfigError::InvalidMtu)));
}

#[test]
fn test_config_validation_mtu_too_large() {
    let config = NetworkConfig {
        mtu: MAX_MTU + 1,
        ..Default::default()
    };
    assert!(matches!(config.validate(), Err(ConfigError::InvalidMtu)));
}

#[test]
fn test_config_validation_timeout_not_greater_than_keepalive() {
    let config = NetworkConfig {
        connection_timeout: Duration::from_secs(1),
        keepalive_interval: Duration::from_secs(1),
        ..Default::default()
    };
    assert!(matches!(
        config.validate(),
        Err(ConfigError::TimeoutNotGreaterThanKeepalive)
    ));
}

#[test]
fn test_config_validation_zero_max_clients() {
    let config = NetworkConfig {
        max_clients: 0,
        ..Default::default()
    };
    assert!(matches!(
        config.validate(),
        Err(ConfigError::InvalidMaxClients)
    ));
}

// ─── Packet roundtrip tests ────────────────────────────────────────────────

#[test]
fn test_packet_roundtrip_keepalive() {
    let header = PacketHeader {
        protocol_id: 0x12345678,
        sequence: 42,
        ack: 41,
        ack_bits: 0xFF,
    };
    let packet = Packet::new(header, PacketType::KeepAlive);
    let data = packet.serialize().unwrap();
    let parsed = Packet::deserialize(&data).unwrap();
    assert_eq!(parsed.header.protocol_id, 0x12345678);
    assert_eq!(parsed.header.sequence, 42);
    assert!(matches!(parsed.packet_type, PacketType::KeepAlive));
}

#[test]
fn test_packet_roundtrip_connection_request() {
    let header = PacketHeader {
        protocol_id: 0xABCD,
        sequence: 0,
        ack: 0,
        ack_bits: 0,
    };
    let packet = Packet::new(header, PacketType::ConnectionRequest);
    let data = packet.serialize().unwrap();
    let parsed = Packet::deserialize(&data).unwrap();
    assert!(matches!(parsed.packet_type, PacketType::ConnectionRequest));
}

#[test]
fn test_packet_roundtrip_challenge() {
    let header = PacketHeader {
        protocol_id: 0x1234,
        sequence: 0,
        ack: 0,
        ack_bits: 0,
    };
    let packet = Packet::new(
        header,
        PacketType::ConnectionChallenge {
            server_salt: 0xDEADBEEFCAFE,
        },
    );
    let data = packet.serialize().unwrap();
    let parsed = Packet::deserialize(&data).unwrap();
    match parsed.packet_type {
        PacketType::ConnectionChallenge { server_salt } => {
            assert_eq!(server_salt, 0xDEADBEEFCAFE);
        }
        _ => panic!("Wrong packet type"),
    }
}

#[test]
fn test_packet_roundtrip_payload_with_data() {
    let header = PacketHeader {
        protocol_id: 0x1234,
        sequence: 5,
        ack: 3,
        ack_bits: 0b111,
    };
    let packet = Packet::new(
        header,
        PacketType::Payload {
            channel: 2,
            is_fragment: true,
        },
    )
    .with_payload(vec![1, 2, 3, 4, 5]);
    let data = packet.serialize().unwrap();
    let parsed = Packet::deserialize(&data).unwrap();
    match parsed.packet_type {
        PacketType::Payload {
            channel,
            is_fragment,
        } => {
            assert_eq!(channel, 2);
            assert!(is_fragment);
        }
        _ => panic!("Wrong packet type"),
    }
    assert_eq!(parsed.payload, vec![1, 2, 3, 4, 5]);
}

#[test]
fn test_packet_roundtrip_disconnect() {
    let header = PacketHeader {
        protocol_id: 0x1234,
        sequence: 0,
        ack: 0,
        ack_bits: 0,
    };
    let packet = Packet::new(header, PacketType::Disconnect { reason: 2 });
    let data = packet.serialize().unwrap();
    let parsed = Packet::deserialize(&data).unwrap();
    match parsed.packet_type {
        PacketType::Disconnect { reason } => assert_eq!(reason, 2),
        _ => panic!("Wrong packet type"),
    }
}

#[test]
fn test_packet_protocol_id_mismatch_detected() {
    let header = PacketHeader {
        protocol_id: 0x1111,
        sequence: 0,
        ack: 0,
        ack_bits: 0,
    };
    let packet = Packet::new(header, PacketType::KeepAlive);
    let data = packet.serialize().unwrap();
    let parsed = Packet::deserialize(&data).unwrap();
    // The packet deserializes fine — it's the consumer's job to check protocol_id
    assert_eq!(parsed.header.protocol_id, 0x1111);
    assert_ne!(parsed.header.protocol_id, 0x12345678);
}

#[test]
fn test_packet_empty_data_rejected() {
    let result = Packet::deserialize(&[]);
    assert!(result.is_err());
}

// ─── Per-message reliability override test ──────────────────────────────────

#[test]
fn test_unreliable_channel_with_reliable_message() {
    let config = ChannelConfig::unreliable();
    let mut ch = Channel::new(0, config);

    // Send with reliable=true on unreliable channel — should track for ACK
    ch.send(b"important", true).unwrap();
    let (_seq, _wire) = ch.get_outgoing_message().unwrap();
    assert_eq!(
        ch.pending_ack_count(),
        1,
        "reliable=true should create pending ACK"
    );
}

#[test]
fn test_reliable_channel_with_unreliable_message() {
    let config = ChannelConfig::reliable_ordered();
    let mut ch = Channel::new(0, config);

    // Send with reliable=false on reliable channel — should NOT track for ACK
    ch.send(b"fire-and-forget", false).unwrap();
    let (_seq, _wire) = ch.get_outgoing_message().unwrap();
    assert_eq!(
        ch.pending_ack_count(),
        0,
        "reliable=false should skip pending ACK"
    );
}
