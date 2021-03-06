//! This module contains the InputStream structure which is used to handle incoming data streams, either TCP, UDP or serial connections.

use tokio::io::{self, AsyncWriteExt, AsyncReadExt};
use tokio::net::{TcpStream, UdpSocket};
use tokio_serial::{SerialPortBuilderExt, SerialStream};
use tokio::sync::{mpsc, broadcast};


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
                
                let socket = TcpStream::connect(&endpoint ).await?;
                let (rd, tx) = io::split(socket);
                let socket = InputSocket::TcpSocket{ip: ip, port: port, rd:  Some(rd), tx: Some(tx)};

                println!("Open TCP listener on {}.", endpoint);
                Ok(socket)
            },
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

                let serial_port = tokio_serial::new(port_name.clone(), baudrate).open_native_async().expect("unable to open serial port");
                let (rd, tx) = io::split(serial_port);
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
        loop {
            let mut buf = vec![0; 1024];

            tokio::select!{
                Some(val) = rx_channel.recv() => {
                    self.write(&val).await.expect("Unexpected MPSC write error");
                },

                Ok(_data) = self.read(&mut buf) => match tx_channel.send(buf) {
                    Ok(_) =>(),
                    Err(_) => ()
                },
            };
        }
    }

}

