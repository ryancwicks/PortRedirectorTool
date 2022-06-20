//! This server listens on a given port and retransmits any data to any connected clients recieved from the broadcast queue.
use tokio::io::{self, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::broadcast;

/// RetransmitServer
///
/// This server runs a TCP server asynchronously and every client will retransmit any data sent to the tx channel. 
///
/// ```rust
/// let (tx, _) = broadcast::channel(32);
/// let tx1 = tx.clone();
///
/// //open the socket and start the reading process.
/// let mut socket_reader = InputSocket::connect(socket_type).await?;
/// tokio::spawn( async move { socket_reader.run_loop(tx).await; });
///
/// // Set up server.
/// let retransmit_server = RetransmitServer(output_port, tx1);
/// tokio::spawn( async move { retransmit_server.run_loop().await; });
/// ```
pub struct RetransmitServer {
    server: TcpListener,
    tx: broadcast::Sender <Vec<u8>> 
}

impl RetransmitServer {

    /// Create a new server that listens to messages broadcase through tx.
    /// This method start the server listening on the given port. Any connected clients will retransmit
    /// any data sent to the tx sender (each instance subscribes to this broadcast sender).
    pub async fn new (port: u16, tx: broadcast::Sender<Vec<u8>>) -> io::Result <RetransmitServer> {
        let server = TcpListener::bind("0.0.0.0:".to_owned() + &port.to_string()).await?;

        println! ("Starting TCP retransmission server at 0.0.0.0:{}", port);

        Ok(RetransmitServer {
            server: server,
            tx: tx
        })
    }

    /// The main run loop. 
    ///
    /// This loop listens for new connections and spawn a new tokio process with a unique reciever.
    /// The spawned processes will simply retransmit the data recieved until the socket or the reciever is closed.
    pub async fn run_loop (&self) { 
        loop {
            //second item contains the ip and port of the new connection
            let (mut client_socket, socket_address) = self.server.accept().await.unwrap();
            let mut rx_client = self.tx.subscribe();
            println!("Accepted client connection at {}", socket_address);
        
            tokio::spawn(async move {
                loop { 
                    let data = rx_client.recv().await; 
                    match data {
                        Ok(data) => {
                            match client_socket.write_all(&data).await {
                                Ok(_) => (),
                                Err(_) => { 
                                    //Socket has been closed, don't sent any more data, close the connection.
                                    println!("Closing client connection on {}", socket_address);
                                    return; 
                                }       
                            }                        
                        }
                        Err(_e) => {
                            //reciever has been closed, no more data will be sent, close the connection.
                            return;
                        }
                    }  
                }
            });
        
        }
        
    }
}





