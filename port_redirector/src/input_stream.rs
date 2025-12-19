//! This module contains the InputStream structure which is used to handle incoming data streams, either TCP, UDP or serial connections.

use tokio::io::{self, AsyncWriteExt, AsyncReadExt};
use tokio::net::{TcpStream, UdpSocket, TcpListener};
use tokio_serial::{SerialPortBuilder, SerialPortBuilderExt, SerialStream, SerialPort};
use tokio::sync::{mpsc, broadcast};
use tokio::time::{sleep, Duration};
use std::sync::atomic::{AtomicU64, Ordering};


/// This enum represents the different input sockets supported by the input connection.
pub enum InputSocket {
    /// The TCP socket requires an ip address and a port. This can either be sent together: 
    /// ```rust
    /// InputSocket::TcpSocket {ip: "192.168.0.1:8080"};
    /// ```
    /// or
    /// ```rust
    /// InputSocket::TcpSocket {ip: "192.168.0.1", port: Some(8080)}; 
    /// ```
    TcpSocket {
        ip: String,
        port: Option<u16>,
        rd: Option<io::ReadHalf<TcpStream>>,
        tx: Option<io::WriteHalf<TcpStream>>
    },
    /// TCP server that listens for a single connection, and only that one connection.
    ///
    TcpServer {
        port: u16,
        server: Option<TcpListener>,
        stream: Option<TcpStream>,
    },
    /// As UDP is stateless, you only need to send a port value.
    /// ```rust
    /// InputSocket::UdpSocket(port: 8080);
    /// ```
    UdpSocket {
        port: u16,
        rd: Option<UdpSocket>
    },
    /// The serial port can be initialized with or without a baudrate. Default is 9600 if a option is not specified.
    /// ```rust
    /// InputSocket::Serial (port_name="COM6", baudrate= Some(115200));
    /// ```
    Serial {
        port_name: String,
        baudrate: Option<u32>,
        rd: Option<io::ReadHalf<SerialStream>>,
        tx: Option<io::WriteHalf<SerialStream>>
    }
}


impl InputSocket {
    /// Create a new input connection given the SocketType and connects to it.
    ///
    /// ```rust
    /// let socket = InputSocket::connect( InputSocket::TcpSocket {ip: "192.168.0.1", port: Some(8080)} )?;
    /// ```
    ///
    /// This will return an error if the connection cannot be made.
    pub async fn connect (port_type: InputSocket) -> io::Result<InputSocket> {

        match port_type {
            InputSocket::TcpSocket {ip, port, ..} => {
                let endpoint = match port {
                    Some(val) => ip.clone() + ":" + &val.to_string(),
                    None => ip.clone()
                };
                
                let socket = TcpStream::connect(&endpoint).await?;
                let (rd, tx) = io::split(socket);
                let socket = InputSocket::TcpSocket{ip: ip, port: port, rd:  Some(rd), tx: Some(tx)};

                println!("Open TCP listener on {}.", endpoint);
                Ok(socket)
            },
            InputSocket::TcpServer {port, ..} => {
                let endpoint = "0.0.0.0:".to_owned() + &port.to_string();

                let server = TcpListener::bind(&endpoint).await?;
                let socket = InputSocket::TcpServer{port: port, server: Some(server), stream: None};
                println!("Input TCP server lisenting on port {}", port);

                Ok(socket)
            }
            InputSocket::UdpSocket {port, ..} => {
                let sock = UdpSocket::bind("0.0.0.0:".to_owned() + &port.to_string()).await?;
                let socket = InputSocket::UdpSocket{port: port, rd:  Some(sock)};
                println!("Open UDP listener on port {}.", port);
                Ok(socket)
            },
            InputSocket::Serial {port_name, baudrate, ..} => {
                let baudrate = match baudrate {
                    Some(val) => val,
                    None => 9600
                };

                let sp_build: SerialPortBuilder = tokio_serial::new(port_name.clone(), baudrate);
                let mut serial_str = match sp_build.open_native_async() {
                    Err(e) => {
                        eprintln!("Error opening {} {}", port_name, e);
                        panic!();
                    },
                    Ok(str) => str
                };
                
                let dtr_ok = serial_str.write_data_terminal_ready(true).is_ok();

                if dtr_ok {
                    println!("DTR Set");
                } else {
                    println!("Error setting DTR (ignored)");
                };

                let (rd, tx) = io::split(serial_str);
                let socket = InputSocket::Serial{port_name: port_name.clone(), baudrate: Some(baudrate), rd: Some(rd), tx: Some(tx)};

                println!("Opened Serial listener on port {} at {} baud.", port_name, baudrate);

                Ok(socket)
            }
        }
    }
    
    
    /// This function allows you to read from the different port types asynchornously.
    /// This follows the convention of the AsyncRead function, returning Ok(0) if the port is closed.
    /// This will also return and error if the reader is uninitialized (with new)
    ///
    /// This function is only used internally by the tokio process spawned by run.
    async fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            InputSocket::TcpSocket {rd, ..} => {
                let rd = match rd {
                    Some(val) => val,
                    None => {return Err(io::Error::new(io::ErrorKind::Other, "Uninitialized TCP reciever."));}
                }; 
                Ok(rd.read(buf).await?)
            },
            InputSocket::TcpServer{server, stream, ..} => {
                // Try to read from existing stream if available
                if let Some(ref mut tcp_stream) = stream {
                    match tcp_stream.read(buf).await {
                        Ok(0) => {
                            // Socket closed, clear it
                            *stream = None;
                            println!("Input client disconnected, waiting for new connection.");
                            return Ok(0);
                        },
                        Ok(n) => {
                            return Ok(n);
                        },
                        Err(e) => {
                            eprintln!("Error reading from tcp server stream: {:?}", e);
                            // Clear the stream on error
                            *stream = None;
                            return Err(e);
                        }
                    }
                }

                // No client connected, accept a new one
                let listener = server.as_ref()
                    .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Uninitialized TCP Server."))?;

                let (new_stream, _addr) = listener.accept().await?;
                println!("TCP Server: New client connected from {}", _addr);
                *stream = Some(new_stream);
                Ok(0)
            },
            InputSocket::UdpSocket {rd, ..} => {
                let rd = match rd {
                    Some(val) => val,
                    None => {return Err(io::Error::new(io::ErrorKind::Other, "Uninitialized UDP reciever."));}
                };
                Ok(rd.recv(buf).await?)
            },
            InputSocket::Serial {rd, ..} => {
                let rd = match rd {
                    Some(val) => val,
                    None => {return Err(io::Error::new(io::ErrorKind::Other, "Uninitialized Serial reciever."));}
                };
                Ok(rd.read(buf).await?)
            }
        }
    }

    /// This function sends data recieved on an MPSC socket.
    async fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            InputSocket::TcpSocket {rd: _, tx, ..} => {
                let tx = match tx {
                    Some(val) => val,
                    None => {return Err(io::Error::new(io::ErrorKind::Other, "Uninitialized TCP transmitter."));}
                }; 
                let length = buf.len();
                tx.write_all(&buf).await?;
                Ok(length)
            },
            InputSocket::TcpServer{stream, ..} => {
                if let Some(ref mut tcp_stream) = stream {
                    let written = buf.len();
                    tcp_stream.write_all(&buf).await?;
                    Ok(written)
                } else {
                    // No client connected, can't write
                    Ok(0)
                }
            }
            InputSocket::UdpSocket {..} => {
                Ok(0)
            },
            InputSocket::Serial {rd: _, tx, ..} => {
                let tx = match tx {
                    Some(val) => val,
                    None => {return Err(io::Error::new(io::ErrorKind::Other, "Uninitialized Serial transmitter."));}
                };
                let length = buf.len();
                tx.write_all(&buf).await?;
                Ok(length)
            }
        }
    }

    pub async fn run_loop (&mut self, tx_channel: broadcast::Sender<Vec<u8>>, mut rx_channel: mpsc::Receiver<Vec<u8>>) {
        // Statistics tracking
        static DROPPED_MESSAGES: AtomicU64 = AtomicU64::new(0);
        static BACKPRESSURE_EVENTS: AtomicU64 = AtomicU64::new(0);

        loop {
            let mut buf = vec![0; 8192];

            tokio::select!{
                Some(val) = rx_channel.recv() => {
                    self.write(&val).await.expect("Unexpected MPSC write error");
                },

                Ok(n) = self.read(&mut buf) => {
                    buf.truncate(n);

                    // Implement backpressure with exponential backoff
                    let mut retry_count = 0;
                    const MAX_RETRIES: u32 = 10;
                    const BASE_DELAY_MS: u64 = 1;

                    loop {
                        match tx_channel.send(buf.clone()) {
                            Ok(_) => {
                                // Successfully sent
                                if retry_count > 0 {
                                    println!("Backpressure resolved after {} retries", retry_count);
                                }
                                break;
                            },
                            Err(broadcast::error::SendError(_)) => {
                                if retry_count == 0 {
                                    // First backpressure event
                                    let bp_count = BACKPRESSURE_EVENTS.fetch_add(1, Ordering::Relaxed) + 1;
                                    eprintln!("WARNING: Broadcast channel full, applying backpressure (event #{})", bp_count);
                                }

                                if retry_count >= MAX_RETRIES {
                                    // Max retries exceeded, drop the message
                                    let dropped = DROPPED_MESSAGES.fetch_add(1, Ordering::Relaxed) + 1;
                                    eprintln!("ERROR: Message dropped after {} retries (total dropped: {})", MAX_RETRIES, dropped);
                                    break;
                                }

                                // Exponential backoff: 1ms, 2ms, 4ms, 8ms, 16ms, 32ms, 64ms, 128ms, 256ms, 512ms
                                let delay_ms = BASE_DELAY_MS * (1 << retry_count);
                                if retry_count % 3 == 0 {
                                    eprintln!("Backpressure: retry {} after {}ms delay", retry_count + 1, delay_ms);
                                }
                                sleep(Duration::from_millis(delay_ms)).await;
                                retry_count += 1;
                            }
                        }
                    }
                },
            };
        }
    }

}

