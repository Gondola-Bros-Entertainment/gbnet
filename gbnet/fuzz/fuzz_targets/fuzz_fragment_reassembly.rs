#![no_main]
use libfuzzer_sys::fuzz_target;
use std::time::Duration;

fuzz_target!(|data: &[u8]| {
    let mut assembler = gbnet::FragmentAssembler::new(Duration::from_secs(5), 64 * 1024);

    // Feed raw bytes as if they were fragment payloads
    // FragmentAssembler should never panic regardless of input
    let _ = assembler.process_fragment(data);

    // Try feeding multiple chunks
    if data.len() >= 2 {
        let mid = data.len() / 2;
        let _ = assembler.process_fragment(&data[..mid]);
        let _ = assembler.process_fragment(&data[mid..]);
    }

    assembler.cleanup();
});
