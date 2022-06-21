//! This server listens on a given port and retransmits any data to any connected clients recieved from the broadcast queue.
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, broadcast};

/// RetransmitServer
///
/// This server runs a TCP server asynchronously and every client will retransmit any data sent to the 
/// tx channel and any data recieved on any socket will be sent on the rx channel. 
///
/// ```rust
/// //Broadcast port for reading in data on the input port and outputting it on all broadcast channels
/// let (broadcast_from_input, _) = broadcast::channel(32);
/// let broadcast_from_input_1 = broadcast_from_input.clone();
///
/// //Mutiple producers to read data in from the server ports and output on the single output port.
/// let (tx_to_input, rx_to_input) = mpsc::channel(32);
///
/// //open the socket and start the reading process.
/// let mut socket_reader = InputSocket::connect(socket_type).await?;
/// tokio::spawn( async move { socket_reader.run_loop(broadcast_from_input, rx_to_input).await; });
///
/// // Set up server.
/// let retransmit_server = RetransmitServer::new(output_port, tx_to_input, broadcast_from_input_1).await?;
/// tokio::spawn( async move { retransmit_server.run_loop().await; });
///
/// ```
pub struct RetransmitServer {
    server: TcpListener,
    tx_to_input: mpsc::Sender <Vec<u8>>,
    broadcast_from_input: broadcast::Sender <Vec<u8>>, 
}

impl RetransmitServer {

    /// Create a new server that listens to messages broadcase through tx.
    /// This method start the server listening on the given port. Any connected clients will retransmit
    /// any data sent to the tx sender (each instance subscribes to this broadcast sender).
    pub async fn new (port: u16, tx_to_input: mpsc::Sender<Vec<u8>>, broadcast_from_input: broadcast::Sender<Vec<u8>>) -> io::Result <RetransmitServer> {
        let server = TcpListener::bind("0.0.0.0:".to_owned() + &port.to_string()).await?;

        println! ("Starting TCP retransmission server at 0.0.0.0:{}", port);

        Ok(RetransmitServer {
            server: server,
            tx_to_input: tx_to_input,
            broadcast_from_input: broadcast_from_input,
        })
    }

    /// The main run loop. 
    ///
    /// This loop listens for new connections and spawn a new tokio process with a unique reciever.
    /// The spawned processes will simply retransmit the data recieved until the socket or the reciever is closed.
    pub async fn run_loop (& self) { 
        loop {
            //second item contains the ip and port of the new connection
            let (mut client_socket, socket_address) = self.server.accept().await.unwrap();
            let mut rx_from_input = self.broadcast_from_input.subscribe();
            let tx_from_client = self.tx_to_input.clone();
            println!("Accepted client connection at {}", socket_address);
        
            tokio::spawn(async move {        
                loop {
                    let mut buf = vec![0; 1024]; 
                    tokio::select! {
                        Ok(data) = rx_from_input.recv() => client_socket.write_all(&data).await.expect("Unexpected socket connection close."),
                        Ok(_data) = client_socket.read(&mut buf) => tx_from_client.send(buf).await.expect("Failed to send data from client to input socket.")
                    };
                }
            });
        
        }
        
    }
}





