// src/tests/serialize_tests.rs - Serialization unit tests

use crate::serialize::{bit_io::BitBuffer, BitDeserialize, BitSerialize};
use gbnet_macros::NetworkSerialize;

#[derive(NetworkSerialize, Debug, PartialEq)]
struct TestPacket {
    #[bits = 6]
    id: u8,
    active: bool,
}

#[test]
fn test_basic_serialization() -> std::io::Result<()> {
    let packet = TestPacket {
        id: 42,
        active: true,
    };

    let mut buffer = BitBuffer::new();
    packet.bit_serialize(&mut buffer)?;

    let bytes = buffer.into_bytes(false)?;
    let mut buffer = BitBuffer::from_bytes(bytes);
    let deserialized = TestPacket::bit_deserialize(&mut buffer)?;

    assert_eq!(packet, deserialized);
    Ok(())
}

#[test]
fn test_primitive_types() -> std::io::Result<()> {
    // Test each primitive type
    let test_u8: u8 = 255;
    let test_u16: u16 = 65535;
    let test_u32: u32 = 0xDEADBEEF;
    let test_bool = true;
    let test_f32: f32 = std::f32::consts::PI;

    // u8
    let mut buffer = BitBuffer::new();
    test_u8.bit_serialize(&mut buffer)?;
    let bytes = buffer.into_bytes(false)?;
    let mut buffer = BitBuffer::from_bytes(bytes);
    assert_eq!(u8::bit_deserialize(&mut buffer)?, test_u8);

    // u16
    let mut buffer = BitBuffer::new();
    test_u16.bit_serialize(&mut buffer)?;
    let bytes = buffer.into_bytes(false)?;
    let mut buffer = BitBuffer::from_bytes(bytes);
    assert_eq!(u16::bit_deserialize(&mut buffer)?, test_u16);

    // u32
    let mut buffer = BitBuffer::new();
    test_u32.bit_serialize(&mut buffer)?;
    let bytes = buffer.into_bytes(false)?;
    let mut buffer = BitBuffer::from_bytes(bytes);
    assert_eq!(u32::bit_deserialize(&mut buffer)?, test_u32);

    // bool
    let mut buffer = BitBuffer::new();
    test_bool.bit_serialize(&mut buffer)?;
    let bytes = buffer.into_bytes(false)?;
    let mut buffer = BitBuffer::from_bytes(bytes);
    assert_eq!(bool::bit_deserialize(&mut buffer)?, test_bool);

    // f32
    let mut buffer = BitBuffer::new();
    test_f32.bit_serialize(&mut buffer)?;
    let bytes = buffer.into_bytes(false)?;
    let mut buffer = BitBuffer::from_bytes(bytes);
    assert_eq!(f32::bit_deserialize(&mut buffer)?, test_f32);

    Ok(())
}

#[test]
fn test_collections() -> std::io::Result<()> {
    // Vec
    let test_vec = vec![1u8, 2, 3, 4, 5];
    let mut buffer = BitBuffer::new();
    test_vec.bit_serialize(&mut buffer)?;
    let bytes = buffer.into_bytes(false)?;
    let mut buffer = BitBuffer::from_bytes(bytes);
    assert_eq!(Vec::<u8>::bit_deserialize(&mut buffer)?, test_vec);

    // String
    let test_string = "Hello, GBNet!".to_string();
    let mut buffer = BitBuffer::new();
    test_string.bit_serialize(&mut buffer)?;
    let bytes = buffer.into_bytes(false)?;
    let mut buffer = BitBuffer::from_bytes(bytes);
    assert_eq!(String::bit_deserialize(&mut buffer)?, test_string);

    // Array
    let test_array: [u32; 4] = [10, 20, 30, 40];
    let mut buffer = BitBuffer::new();
    test_array.bit_serialize(&mut buffer)?;
    let bytes = buffer.into_bytes(false)?;
    let mut buffer = BitBuffer::from_bytes(bytes);
    assert_eq!(<[u32; 4]>::bit_deserialize(&mut buffer)?, test_array);

    // Option
    let test_some: Option<u16> = Some(12345);
    let test_none: Option<u16> = None;

    let mut buffer = BitBuffer::new();
    test_some.bit_serialize(&mut buffer)?;
    let bytes = buffer.into_bytes(false)?;
    let mut buffer = BitBuffer::from_bytes(bytes);
    assert_eq!(Option::<u16>::bit_deserialize(&mut buffer)?, test_some);

    let mut buffer = BitBuffer::new();
    test_none.bit_serialize(&mut buffer)?;
    let bytes = buffer.into_bytes(false)?;
    let mut buffer = BitBuffer::from_bytes(bytes);
    assert_eq!(Option::<u16>::bit_deserialize(&mut buffer)?, test_none);

    Ok(())
}

#[test]
fn test_bit_packing() -> std::io::Result<()> {
    #[derive(NetworkSerialize, Debug, PartialEq)]
    struct BitPacked {
        #[bits = 3]
        small: u8, // 0-7
        #[bits = 5]
        medium: u8, // 0-31
        #[bits = 1]
        flag: bool,
    }

    let packed = BitPacked {
        small: 7,
        medium: 31,
        flag: true,
    };

    let mut buffer = BitBuffer::new();
    packed.bit_serialize(&mut buffer)?;

    // Should only use 9 bits total
    assert_eq!(buffer.unpadded_length(), 9);

    let bytes = buffer.into_bytes(false)?;
    let mut buffer = BitBuffer::from_bytes(bytes);
    let deserialized = BitPacked::bit_deserialize(&mut buffer)?;

    assert_eq!(packed, deserialized);
    Ok(())
}
