#![no_main]
use libfuzzer_sys::fuzz_target;
use gbnet::{BitBuffer, BitDeserialize};

fuzz_target!(|data: &[u8]| {
    let mut buf = BitBuffer::from_bytes(data.to_vec());

    // Try reading various types — should never panic
    let _ = u8::bit_deserialize(&mut buf);
    let _ = u16::bit_deserialize(&mut buf);
    let _ = u32::bit_deserialize(&mut buf);
    let _ = bool::bit_deserialize(&mut buf);
    let _ = f32::bit_deserialize(&mut buf);
});
