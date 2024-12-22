use crate::message::{client_message, server_message, AddRequest, AddResponse, ClientMessage, EchoMessage, ServerMessage};
use log::{error, info, warn};
use prost::Message;
use std::{
    io::{self, ErrorKind, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

struct Client {
    stream: TcpStream,
}

impl Client {
    pub fn new(stream: TcpStream) -> Self {
        Client { stream }
    }

    pub fn handle(&mut self) -> io::Result<bool> {  //changed return type to include connection status
        let mut buffer = [0; 1024];
        // Try to decode as a ClientMessage
        
        // Read data from the client
        match self.stream.read(&mut buffer){
            Ok(0)=>{
                //connection closed by the client
                return Ok(false);
            }
            Ok(bytes_read)=>{
                if let Ok(client_msg)=ClientMessage::decode(&buffer[..bytes_read]){
                    match client_msg.message{
                        Some(client_message::Message::EchoMessage(echo))=>{
                            info!("Received Echo: {}", echo.content);
                            // Send Echo response
                            let response=ServerMessage{
                                message:Some(server_message::Message::EchoMessage(echo)),
                            };
                            let payload = response.encode_to_vec();
                            self.stream.write_all(&payload)?;
                            self.stream.flush()?;
                        }
                        Some(client_message::Message::AddRequest(add))=>{
                            info!("recieved add request:{} + {}",add.a,add.b);
                            //calculate result and create response
                            let result=add.a+add.b;
                            let response=ServerMessage{
                                message: Some(server_message::Message::AddResponse(AddResponse{
                                    result
                                })),                                
                            };
                            let payload = response.encode_to_vec();
                            self.stream.write_all(&payload)?;
                            self.stream.flush()?;
                        }
                    
                        None =>{
                            error!("Received empty message");
                        }
                    }
                }
                    else{
                        error!("Failed to decode message");
                    }
                    Ok(true)
                    }
                    
                Err(ref e)if e.kind()==ErrorKind::WouldBlock=>{
                        // no data availabe 
                        Ok(true)
                }
                Err(e)=>{
                    //other errors
                    Err(e)
                }
        }
    }
}

pub struct Server {
    listener: TcpListener,
    is_running: Arc<AtomicBool>,
}

impl Server {
    /// Creates a new server instance
    pub fn new(addr: &str) -> io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        let is_running = Arc::new(AtomicBool::new(false));
        Ok(Server {
            listener,
            is_running,
        })
    }

    /// Runs the server, listening for incoming connections and handling them
    pub fn run(&self) -> io::Result<()> {
        self.is_running.store(true, Ordering::SeqCst); // Set the server as running
        info!("Server is running on {}", self.listener.local_addr()?);

        // Set the listener to non-blocking mode
        self.listener.set_nonblocking(true)?;

        while self.is_running.load(Ordering::SeqCst) {
            match self.listener.accept() {
                Ok((stream, addr)) => {
                    info!("New client connected: {}", addr);

                    stream.set_nonblocking(true)?;//set the client stream to non blocking
                    
                    //create a new arc clone for this client's thread 
                    let is_running=Arc::clone(&self.is_running);
                    //spawn a new thread for this client
                    thread::spawn(move||{
                        let mut client = Client::new(stream);
                        while is_running.load(Ordering::SeqCst) {
                            match client.handle(){
                                Ok(true)=>{
                                    //connection still alive
                                    thread::sleep(Duration::from_millis(10));
                                }
                                Ok(false)=>{
                                    //client disconnected
                                    info!("Client disconnected");
                                    break;
                                }
                                Err(e)=>{
                                    error!("Error handling client {}: {}",addr,e);
                                    break;
                                }
                            }
                        }
                        info!("Client handler thread for {} stopped",addr);
                    });
                    
                    
                }
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    // No incoming connections, sleep briefly to reduce CPU usage
                    thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    error!("Error accepting connection: {}", e);
                }
            }
        }

        info!("Server stopped.");
        Ok(())
    }

    /// Stops the server by setting the `is_running` flag to `false`
    pub fn stop(&self) {
        if self.is_running.load(Ordering::SeqCst) {
            self.is_running.store(false, Ordering::SeqCst);
            info!("Shutdown signal sent.");
        } else {
            warn!("Server was already stopped or not running.");
        }
    }
}