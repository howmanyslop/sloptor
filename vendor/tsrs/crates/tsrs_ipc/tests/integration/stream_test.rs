use std::io::{Read, Write};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use tsrs_ipc::tcp_pipe;

#[test]
fn tcp_pipe_exchanges_data_in_both_directions() {
    let (mut client, mut server) = tcp_pipe().unwrap();
    client.writer.write_all(b"request").unwrap();
    let mut request = [0; 7];
    server.reader.read_exact(&mut request).unwrap();
    assert_eq!(&request, b"request");

    server.writer.write_all(b"response").unwrap();
    let mut response = [0; 8];
    client.reader.read_exact(&mut response).unwrap();
    assert_eq!(&response, b"response");
}

#[test]
fn tcp_pipe_close_releases_both_readers_and_is_idempotent() {
    let (mut client, mut server) = tcp_pipe().unwrap();
    let (done, received) = mpsc::channel();
    let peer_done = done.clone();
    let client_thread = thread::spawn(move || {
        done.send(client.reader.read(&mut [0])).unwrap();
    });
    let server_thread = thread::spawn(move || {
        peer_done.send(server.reader.read(&mut [0])).unwrap();
    });

    client.closer.close().unwrap();
    client.closer.close().unwrap();
    for _ in 0..2 {
        let result = received.recv_timeout(Duration::from_secs(10)).expect("reader stayed blocked");
        assert!(matches!(result, Ok(0) | Err(_)), "unexpected data: {result:?}");
    }
    assert!(client.writer.write_all(b"closed").is_err());
    client_thread.join().unwrap();
    server_thread.join().unwrap();
}
