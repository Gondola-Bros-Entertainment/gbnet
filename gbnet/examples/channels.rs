//! Demonstrates all 5 delivery modes by creating channels and sending
//! messages through each one.
//!
//! Run with: `cargo run --example channels`

use gbnet::{Channel, ChannelConfig};

fn main() {
    let configs = [
        ChannelConfig::unreliable(),
        ChannelConfig::unreliable_sequenced(),
        ChannelConfig::reliable_unordered(),
        ChannelConfig::reliable_ordered(),
        ChannelConfig::reliable_sequenced(),
    ];

    for (i, config) in configs.iter().enumerate() {
        let name = format!("{:?}", config.delivery_mode);
        let mut sender = Channel::new(i as u8, *config);
        let mut receiver = Channel::new(i as u8, *config);

        // Send 3 messages
        for j in 0u8..3 {
            let msg = format!("{:?} message {}", config.delivery_mode, j);
            let reliable = config.delivery_mode.is_reliable();
            sender.send(msg.as_bytes(), reliable).unwrap();
        }

        // Transfer wire data from sender to receiver
        while let Some((_seq, wire)) = sender.get_outgoing_message() {
            receiver.on_packet_received(wire);
        }

        // Drain received messages
        print!("{:<22} received: ", name);
        let mut msgs = Vec::new();
        while let Some(data) = receiver.receive() {
            msgs.push(String::from_utf8_lossy(&data).to_string());
        }
        println!("{}", msgs.join(", "));
    }
}
