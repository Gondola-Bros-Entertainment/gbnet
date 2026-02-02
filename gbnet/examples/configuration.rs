//! Demonstrates custom NetworkConfig with multiple channel types,
//! delivery mode tuning, and connection parameters.
//!
//! Run with: `cargo run --example configuration`

use gbnet::{ChannelConfig, NetworkConfig};
use std::time::Duration;

fn main() {
    // Build a config suitable for a fast-paced action game
    let config = NetworkConfig::default()
        .with_protocol_id(0xDEADBEEF)
        .with_max_clients(32)
        .with_mtu(1200)
        .with_connection_timeout(Duration::from_secs(10))
        .with_keepalive_interval(Duration::from_secs(1))
        .with_send_rate(60.0)
        .with_max_in_flight(256)
        // Channel 0: game events — reliable, in-order, highest priority
        .with_channel_config(0, ChannelConfig::reliable_ordered().with_priority(0))
        // Channel 1: position updates — unreliable, lowest priority
        .with_channel_config(1, ChannelConfig::unreliable().with_priority(255))
        // Channel 2: voice/chat — reliable, unordered, medium priority
        .with_channel_config(2, ChannelConfig::reliable_unordered().with_priority(128))
        // Channel 3: config sync — reliable, only latest matters
        .with_channel_config(3, ChannelConfig::reliable_sequenced().with_priority(64));

    // Validate before use
    config.validate().expect("Config should be valid");

    println!("Network Configuration:");
    println!("  Protocol ID:      0x{:08X}", config.protocol_id);
    println!("  Max clients:      {}", config.max_clients);
    println!("  MTU:              {} bytes", config.mtu);
    println!("  Fragment at:      {} bytes", config.fragment_threshold);
    println!("  Send rate:        {} Hz", config.send_rate);
    println!("  Max in-flight:    {}", config.max_in_flight);
    println!("  Keepalive:        {:?}", config.keepalive_interval);
    println!("  Timeout:          {:?}", config.connection_timeout);
    println!(
        "  Disconnect retry: {} attempts, {:?} interval",
        config.disconnect_retries, config.disconnect_retry_timeout
    );
    println!(
        "  Rate limit:       {} req/s per IP",
        config.rate_limit_per_second
    );
    println!();

    println!("Channels:");
    for (i, ch_cfg) in config.channel_configs.iter().enumerate() {
        println!(
            "  Channel {}: {:?} (priority {}, max msg {} bytes)",
            i, ch_cfg.delivery_mode, ch_cfg.priority, ch_cfg.max_message_size
        );
    }
}
