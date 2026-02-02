#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Should never panic on arbitrary input
    let _ = gbnet::Packet::deserialize(data);
});
