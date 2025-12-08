//! This port_redirector_tool is the console application that opens a single port and retransmits it's data to multiple TCP sockets./
use port_redirector::input_stream::InputSocket;
use port_redirector::retransmit_server::RetransmitServer;

use tokio::io;
use tokio::signal;
use tokio::sync::{mpsc, broadcast};
use clap::{Arg, Command};
use std::fmt::Write;

/// This program opens the provided port (either TCP, UDP or Serial), starts a TCP server, and retransmits any data 
/// given to that port to any client connected to that server. It will also read in data from the server and retransmit on the single port.
/// 
/// To get a help message, run with the -h flag.
/// 
/// To exit the program, type Ctrl-C
#[tokio::main]
async fn main() -> io::Result<()> {
   
    //Parse the input arguments.
    let matches = Command::new ("port_redirector_tool")
        .about(
"This application takes input from a UDP, Serial or TCP port and redirects out on a TCP server that multiple clients can connect to.
\tUsage: TCP input:
\t\tport_redirector_tool -t tcp -e 192.168.42.110 -p 5001 -o 8001\n
\n The above command will open up TCP port 5001 on 192.168.42.110 and locally serve whatever it reads to TCP clients that connect to localhost 8001.\n
\tUDP Input:
\t\tport_redirector_tool -t udp -p 5001 -o 8001\n
 The above command will open up the local port 5001 with UDP and retransmit any UDP data sent to it through to clients that connect to localhost 8001. \n
\tSerial Input:
\t\t port_redirector_tool -t serial -e COM6 -b 115200 -o 8001\n
The above command will open the serial port on COM6 at 115200 baud and retransmit any data recieved to clients connected to the TCP server at localhost 8001. \n" )
        .arg(Arg::new("type")
                    .short('t')
                    .long("type")
                    .value_name("TYPE")
                    .required(true)
                    .help("What type of input: 'Serial', 'TCP', 'UDP'"))
        .arg(Arg::new("endpoint")
                    .short('e')
                    .long("endpoint")
                    .value_name("ENDPOINT")
                    .help("What endpoint to listen too (<ip> for TCP or <com port> for Serial"))
        .arg(Arg::new("port")
                    .short('p')
                    .long("port")
                    .value_name("PORT")
                    .help("What port to listen on (UDP and TCP)"))
        .arg(Arg::new("baudrate")
                    .short('b')
                    .long("baudrate")
                    .value_name("BAUDRATE")
                    .help("Baudrate for the serial port (default 9600)"))
        .arg(Arg::new("output_port")
                    .short('o')
                    .long("output_port")
                    .value_name("OUTPUT_PORT")
                    .required(true)
                    .help("What port to listen on for the TCP redirector server."))
        .get_matches();


    let output_port = matches.get_one::<String>("output_port")
        .expect("output_port is required")
        .parse::<u16>()
        .expect("output_port must be a valid u16");
    let socket_type_name = matches.get_one::<String>("type")
        .expect("type is required")
        .to_ascii_lowercase();


    let socket_type = match socket_type_name.as_str() {
        "missing" => {return Err(io::Error::new(io::ErrorKind::Other, "Missing parameter socket type name."));}
        "tcp" => {
            let ip = matches.get_one::<String>("endpoint")
                .expect("Endpoint IP address required for TCP")
                .to_string();
            let port = matches.get_one::<String>("port")
                .expect("Port required for TCP")
                .parse::<u16>()
                .expect("Port must be a valid u16");
            InputSocket::TcpSocket { ip: ip.to_string(), port: Some(port), rd: None, tx: None }
        }
        "udp" => {
            let port = matches.get_one::<String>("port")
                .expect("Port required for UDP")
                .parse::<u16>()
                .expect("Port must be a valid u16");
            InputSocket::UdpSocket {port: port, rd: None}
        }
        "serial" => {
            let port_name = matches.get_one::<String>("endpoint")
                .expect("Serial port name required")
                .to_string();
            let baudrate = matches.get_one::<String>("baudrate")
                .expect("Baudrate required for Serial")
                .parse::<u32>()
                .expect("Baudrate must be a valid u32");
            InputSocket::Serial {port_name: port_name, baudrate: Some(baudrate), rd: None, tx: None}
        }
        _ =>  { 
            let mut err_str = String::new();
            writeln! (err_str, "Invalid parameter socket type name: {}", socket_type_name).unwrap();
            return Err(io::Error::new(io::ErrorKind::Other, err_str ));
        }
    };


    //Broadcast port for reading in data on the input port and outputting it on all broadcast channels
    let (broadcast_from_input_tx, broadcast_from_input_rx) = broadcast::channel(32);

    //Mutiple producers to read data in from the server ports and output on the single output port.
    let (tx_to_input, rx_to_input) = mpsc::channel(32);

    //open the socket and start the reading process.
    let mut socket_reader = InputSocket::connect(socket_type).await?;
    tokio::spawn( async move { socket_reader.run_loop(broadcast_from_input_tx, rx_to_input).await; });

    // Set up server.
    let mut retransmit_server = RetransmitServer::new(output_port, tx_to_input, broadcast_from_input_rx).await?;
    tokio::spawn( async move { retransmit_server.run_loop().await; });

    match signal::ctrl_c().await {
        Ok(()) => {},
        Err(err) => {
            eprintln!("Unable to listen for shutdown signal: {}", err);
            // we also shut down in case of error
        },
    }

    Ok(())
}