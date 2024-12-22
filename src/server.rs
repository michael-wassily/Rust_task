use crate::message::{client_message, server_message, AddRequest, AddResponse, ClientMessage, EchoMessage, ServerMessage};
use log::{error, info, warn};
use prost::Message;
use std::{
    io::{self, ErrorKind, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,Mutex,
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
            Ok(0)=>return Ok(false),//connection closed by the client

            Ok(bytes_read)=>{
                match ClientMessage::decode(&buffer[..bytes_read]){
                    Ok(client_msg)=>{
                        match client_msg.message{
                            Some(client_message::Message::EchoMessage(echo))=>{
                                info!("Received Echo: {}", echo.content);
                                // Send Echo response
                                self.handle_echo(echo)
                            }
                            Some(client_message::Message::AddRequest(add))=>{
                                info!("recieved add request:{} + {}",add.a,add.b);
                                //calculate result and create response
                                self.handle_add(add)
                                
                            }
                        
                            None =>{
                                error!("Received empty message");
                                Ok(true)
                            }
                        }
                    }
                    Err(e)=>{
                        error!("Failed to decode message:{}",e);
                        Ok(true)
                    }
                    
                }
            }
                    
            Err(ref e)if e.kind()==ErrorKind::WouldBlock=>Ok(true),// no data availabel
            Err(e)=>Err(e),//other errors

        }
    }
    fn handle_echo(&mut self,echo:EchoMessage)->io::Result<bool>{
        let response=ServerMessage{
            message:Some(server_message::Message::EchoMessage(echo)),
        };
        self.send_response(response)
    }
    fn handle_add(&mut self,add:AddRequest)->io::Result<bool>{
        let result=add.a+add.b;
         let response=ServerMessage{
            message: Some(server_message::Message::AddResponse(AddResponse{
              result
             })),                              
         };
          self.send_response(response)  
    }
    fn send_response(&mut self,response:ServerMessage)->io::Result<bool>{
        let payload = response.encode_to_vec();
        self.stream.write_all(&payload)?;
        self.stream.flush()?;
        Ok(true)
    }
}

pub struct Server {
    listener: TcpListener,
    is_running: Arc<AtomicBool>,
    state: Arc<Mutex<ServerState>>,//add shared state for data consistancy and race conditions
}
pub struct ServerState{
    connection_count:i32,
}
impl Server {
    /// Creates a new server instance
    pub fn new(addr: &str) -> io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        let is_running = Arc::new(AtomicBool::new(false));
        let state=Arc::new(Mutex::new(ServerState{
            connection_count:0,
        }));
        Ok(Server {
            listener,
            is_running,
            state,
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

                    //update connection count safely
                    {
                        let mut state=self.state.lock().unwrap();
                        state.connection_count+=1;
                        info!("Active connections: {}",state.connection_count);
                    }

                    stream.set_nonblocking(true)?;//set the client stream to non blocking
                    
                    //create a new arc clone for this client's thread 
                    let is_running=Arc::clone(&self.is_running);
                    //clone the state Arc of the thread
                    let thread_state=Arc::clone(&self.state);
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
                        //decrease connection count when disconnected 
                        let mut state= thread_state.lock().unwrap();
                        state.connection_count-=1;
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