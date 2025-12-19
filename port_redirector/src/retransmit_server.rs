//! This server listens on a given port and retransmits any data to any connected clients recieved from the broadcast queue.
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc};
use tokio::time::{timeout, Duration};
use std::collections::VecDeque;

/// RetransmitServer
///
/// This server runs a TCP server asynchronously and every client will retransmit any data sent to the
/// tx channel and any data recieved on any socket will be sent on the rx channel.
///
/// ```rust
/// //Broadcast port for reading in data on the input port and outputting it on all broadcast channels
/// let (broadcast_from_input_tx, broadcast_from_input_rx) = broadcast::channel(32);
///
/// //Mutiple producers to read data in from the server ports and output on the single output port.
/// let (tx_to_input, rx_to_input) = mpsc::channel(32);
///
/// //open the socket and start the reading process.
/// let mut socket_reader = InputSocket::connect(socket_type).await?;
/// tokio::spawn( async move { socket_reader.run_loop(broadcast_from_input_tx, rx_to_input).await; });
///
/// // Set up server.
/// let mut retransmit_server = RetransmitServer::new(output_port, tx_to_input, broadcast_from_input_rx).await?;
/// tokio::spawn( async move { retransmit_server.run_loop().await; });
///
/// ```
pub struct RetransmitServer {
    server: TcpListener,
    tx_to_input: mpsc::Sender<Vec<u8>>,
    broadcast_from_input_rx: broadcast::Receiver<Vec<u8>>,
}

impl RetransmitServer {
    /// Create a new server that listens to messages broadcase through tx.
    /// This method start the server listening on the given port. Any connected clients will retransmit
    /// any data sent to the tx sender (each instance subscribes to this broadcast sender).
    pub async fn new(
        port: u16,
        tx_to_input: mpsc::Sender<Vec<u8>>,
        broadcast_from_input_rx: broadcast::Receiver<Vec<u8>>,
    ) -> io::Result<RetransmitServer> {
        let server = TcpListener::bind("0.0.0.0:".to_owned() + &port.to_string()).await?;

        println!(
            "Starting TCP output retransmission server at 0.0.0.0:{}",
            port
        );

        Ok(RetransmitServer {
            server: server,
            tx_to_input: tx_to_input,
            broadcast_from_input_rx: broadcast_from_input_rx,
        })
    }

    /// The main run loop.
    ///
    /// This loop listens for new connections and spawn a new tokio process with a unique reciever.
    /// The spawned processes will simply retransmit the data recieved until the socket or the reciever is closed.
    pub async fn run_loop(&mut self) {
        loop {
            //second item contains the ip and port of the new connection
            let (mut client_socket, socket_address) = self.server.accept().await.unwrap();
            let mut rx_from_input = self.broadcast_from_input_rx.resubscribe();
            let tx_from_client = self.tx_to_input.clone();
            println!("Accepted output client connection at {}", socket_address);

            tokio::spawn(async move {
                // Per-client buffer to handle temporary slow writes
                let mut pending_writes: VecDeque<Vec<u8>> = VecDeque::new();
                const MAX_PENDING_WRITES: usize = 100;
                const WRITE_TIMEOUT_MS: u64 = 5000; // 5 second timeout for writes
                let mut slow_client_warnings = 0;

                loop {
                    let mut buf = vec![0; 8192];

                    // Try to flush pending writes first
                    while let Some(data) = pending_writes.front() {
                        match timeout(Duration::from_millis(WRITE_TIMEOUT_MS), client_socket.write_all(data)).await {
                            Ok(Ok(())) => {
                                pending_writes.pop_front();
                            },
                            Ok(Err(e)) => {
                                eprintln!("Client {} disconnected (write error: {})", socket_address, e);
                                return;
                            },
                            Err(_) => {
                                slow_client_warnings += 1;
                                if slow_client_warnings % 5 == 1 {
                                    eprintln!("WARNING: Client {} is slow (timeout #{}, {} messages pending)",
                                             socket_address, slow_client_warnings, pending_writes.len());
                                }
                                if slow_client_warnings > 10 {
                                    eprintln!("ERROR: Client {} too slow, disconnecting", socket_address);
                                    return;
                                }
                                break; // Move on to handle other events
                            }
                        }
                    }

                    tokio::select! {
                        Ok(data) = rx_from_input.recv() => {
                            // Try to write immediately if no pending writes
                            if pending_writes.is_empty() {
                                match timeout(Duration::from_millis(WRITE_TIMEOUT_MS), client_socket.write_all(&data)).await {
                                    Ok(Ok(())) => {
                                        // Successfully written
                                    },
                                    Ok(Err(_)) => {
                                        println!("Client {} disconnected (write failed)", socket_address);
                                        break;
                                    },
                                    Err(_) => {
                                        // Timeout, buffer the message
                                        eprintln!("WARNING: Client {} write timeout, buffering message", socket_address);
                                        pending_writes.push_back(data);
                                    }
                                }
                            } else {
                                // Already have pending writes, add to buffer
                                if pending_writes.len() >= MAX_PENDING_WRITES {
                                    eprintln!("ERROR: Client {} buffer full ({} messages), disconnecting",
                                             socket_address, MAX_PENDING_WRITES);
                                    break;
                                }
                                pending_writes.push_back(data);
                            }
                        },
                        result = client_socket.read(&mut buf) => {
                            match result {
                                Ok(0) => {
                                    println!("Input Client {} disconnected (connection closed)", socket_address);
                                    break;
                                },
                                Ok(n) => {
                                    buf.truncate(n);
                                    if let Err(_) = tx_from_client.send(buf).await {
                                        println!("Failed to send data from client {} to input socket", socket_address);
                                        break;
                                    }
                                },
                                Err(_) => {
                                    println!("Client {} disconnected (read error)", socket_address);
                                    break;
                                }
                            }
                        }
                    };
                }
            });
        }
    }
}
