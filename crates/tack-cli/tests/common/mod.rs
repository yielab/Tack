//! Shared fixture for this crate's real-subprocess tests.

/// Binds an ephemeral port and immediately drops the listener so a spawned
/// subprocess can bind it next — the same small, accepted race window this
/// crate's real-subprocess tests have always carried.
pub fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    listener.local_addr().expect("local addr").port()
}
