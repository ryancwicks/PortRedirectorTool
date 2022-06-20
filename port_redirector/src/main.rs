//! This port_redirector_tool is the console application that opens a single port and retransmits it's data to multiple TCP sockets./
use port_redirector::input_stream::InputSocket;
use port_redirector::retransmit_server::RetransmitServer;

use tokio::io;
use tokio::signal;
use tokio::sync::broadcast;
use clap::{Arg, App, value_t};
use std::fmt::Write;

/// This program opens the provided port (either TCP, UDP or Serial), starts a TCP server, and retransmits any data 
/// given to that port to any client connected to that server.
/// 
/// To get a help message, run with the -h flag.
/// 
/// To exit the program, type Ctrl-C
#[tokio::main]
async fn main() -> io::Result<()> {
   
    //Parse the input arguments.
    let matches = App::new ("port_redirector_tool")
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
        .arg(Arg::with_name("type")
                    .short("t")
                    .long("type")
                    .takes_value(true)
                    .required(true)
                    .help("What type of input: 'Serial', 'TCP', 'UDP'"))
        .arg(Arg::with_name("endpoint")
                    .short("e")
                    .long("endpoint")
                    .takes_value(true)
                    .help("What endpoint to listen too (<ip> for TCP or <com port> for Serial"))
        .arg(Arg::with_name("port")
                    .short("p")
                    .long("port")
                    .takes_value(true)
                    .help("What port to listen on (UDP and TCP)"))
        .arg(Arg::with_name("baudrate")
                    .short("b")
                    .long("baudrate")
                    .takes_value(true)
                    .help("Baudrate for the serial port (default 9600)"))
        .arg(Arg::with_name("output_port")
                    .short("o")
                    .long("output_port")
                    .takes_value(true)
                    .required(true)
                    .help("What port to listen on for the TCP redirector server."))
        .get_matches();


    let output_port = clap::value_t!(matches, "output_port", u16).unwrap_or_else(|e| {
        println!("{}",e); 
        e.exit();
    });
    let socket_type_name = matches.value_of("type").unwrap().to_ascii_lowercase(); //guaranteed to exist due to CLAP's required flag.


    let socket_type = match socket_type_name.as_str() {
        "missing" => {return Err(io::Error::new(io::ErrorKind::Other, "Missing parameter socket type name."));}
        "tcp" => {
            let ip = clap::value_t! (matches, "endpoint", String).unwrap_or_else (|e| {
                println!("Enpoint IP address required: {}",e); 
                e.exit();
            });
            let port = clap::value_t!(matches, "port", u16).unwrap_or_else(|e| {
                println!("Port required: {}",e); 
                e.exit();
            });
            InputSocket::TcpSocket { ip: ip.to_string(), port: Some(port), rd: None }
        }
        "udp" => {
            let port = clap::value_t!(matches, "port", u16).unwrap_or_else(|e| {
                println!("Port required: {}",e); 
                e.exit();
            });
            InputSocket::UdpSocket {port: port, rd: None}
        }
        "serial" => {
            let port_name = clap::value_t! (matches, "endpoint", String).unwrap_or_else (|e| {
                println!("Serial port name required: {}",e); 
                e.exit();
            });
            let baudrate = clap::value_t!(matches, "baudrate", u32).unwrap_or_else(|e| {
                println!("Baudrate required: {}",e); 
                e.exit();
            });
            InputSocket::Serial {port_name: port_name, baudrate: Some(baudrate), rd: None}
        }
        _ =>  { 
            let mut err_str = String::new();
            writeln! (err_str, "Invalid parameter socket type name: {}", socket_type_name).unwrap();
            return Err(io::Error::new(io::ErrorKind::Other, err_str ));
        }
    };


    let (tx, _) = broadcast::channel(32);
    let tx1 = tx.clone();

    //open the socket and start the reading process.
    let mut socket_reader = InputSocket::connect(socket_type).await?;
    tokio::spawn( async move { socket_reader.run_loop(tx).await; });

    // Set up server.
    let retransmit_server = RetransmitServer::new(output_port, tx1).await?;
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