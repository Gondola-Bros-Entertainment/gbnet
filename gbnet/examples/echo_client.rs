//! Echo client example — connects to the echo server, sends a message, and
//! prints the response.
//!
//! Run with: `cargo run --example echo_client`

use gbnet::prelude::*;
use std::time::Duration;

fn main() {
    let server_addr: SocketAddr = "127.0.0.1:7777".parse().unwrap();
    let mut client =
        NetClient::connect(server_addr, NetworkConfig::default()).expect("Failed to connect");
    println!("Connecting to {}...", server_addr);

    let mut sent = false;

    loop {
        for event in client.update() {
            match event {
                ClientEvent::Connected => {
                    println!("[+] Connected to server");
                }
                ClientEvent::Disconnected(reason) => {
                    println!("[-] Disconnected: {:?}", reason);
                    return;
                }
                ClientEvent::Message { channel, data } => {
                    println!(
                        "[<] Echo reply on channel {}: {:?}",
                        channel,
                        String::from_utf8_lossy(&data)
                    );
                    client.disconnect();
                    return;
                }
            }
        }

        if client.is_connected() && !sent {
            let msg = b"Hello from GB-Net!";
            client.send(0, msg).expect("Failed to send");
            println!("[>] Sent: {:?}", String::from_utf8_lossy(msg));
            sent = true;
        }

        std::thread::sleep(Duration::from_millis(16));
    }
}
