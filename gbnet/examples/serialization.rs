//! Demonstrates bitpacked serialization using `#[derive(NetworkSerialize)]`
//! with `#[bits = N]` field attributes.
//!
//! Run with: `cargo run --example serialization`

use gbnet::{BitBuffer, BitDeserialize, BitSerialize, BitWrite, NetworkSerialize};

#[derive(NetworkSerialize, Debug, PartialEq)]
struct PlayerState {
    #[bits = 16]
    player_id: u16,
    #[bits = 10]
    x: u16, // 0..1023
    #[bits = 10]
    y: u16, // 0..1023
    #[bits = 8]
    health: u8,
    #[bits = 1]
    crouching: bool,
    #[bits = 3]
    weapon_slot: u8, // 0..7
}

fn main() {
    let state = PlayerState {
        player_id: 42,
        x: 512,
        y: 768,
        health: 100,
        crouching: true,
        weapon_slot: 3,
    };

    // Serialize
    let mut buffer = BitBuffer::new();
    state.bit_serialize(&mut buffer).unwrap();
    let total_bits = BitWrite::bit_pos(&buffer);
    let bytes = buffer.into_bytes(true).unwrap();

    println!("PlayerState: {:?}", state);
    println!(
        "Serialized: {} bits ({} bytes on wire)",
        total_bits,
        bytes.len()
    );
    println!("Raw bytes: {:02x?}", bytes);

    // Deserialize
    let mut read_buffer = BitBuffer::from_bytes(bytes);
    let decoded = PlayerState::bit_deserialize(&mut read_buffer).unwrap();

    println!("Deserialized: {:?}", decoded);
    assert_eq!(state, decoded);
    println!("Round-trip OK!");
}
