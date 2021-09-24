use std::{
    collections::HashMap,
    net::{SocketAddr, TcpListener, TcpStream},
    ops::Deref,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{channel, Sender},
        Arc, Mutex,
    },
    thread::{sleep, JoinHandle},
    time::Duration,
};

use log::{info, warn};
use socket2::{Domain, Protocol, Socket, Type};

use crate::{
    error::{Error, Result},
    msg::{Message, MessageDecoder, MessageEncoder, MessageFormat},
};

pub struct Server {
    fmt: MessageFormat,

    stop_flag: Arc<AtomicBool>,

    listen_addr: Option<String>,
    tx_map: Arc<Mutex<HashMap<String, Sender<Message>>>>,

    handle: Option<JoinHandle<()>>,
}

impl Server {
    pub fn new(fmt: MessageFormat) -> Self {
        Self {
            fmt,
            stop_flag: Arc::new(AtomicBool::new(false)),
            listen_addr: None,
            tx_map: Default::default(),
            handle: None,
        }
    }

    pub fn listen_addr(&self) -> &Option<String> {
        &self.listen_addr
    }

    pub fn client_len(&self) -> usize {
        self.tx_map.lock().unwrap().len()
    }

    pub fn run(&mut self, listen_addr: Option<&str>) -> Result<()> {
        let listen_addr = listen_addr.unwrap_or("127.0.0.1:0");

        let listen_addr: SocketAddr = listen_addr.parse().map_err(|_| Error::AddrParse {
            invalid_addr: listen_addr.to_string(),
        })?;

        let socket =
            Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP)).map_err(Error::Io)?;
        socket.set_nonblocking(true).map_err(Error::Io)?;
        socket.bind(&listen_addr.into()).map_err(Error::Io)?;
        socket.listen(2).map_err(Error::Io)?;

        let listen_addr = socket.local_addr().unwrap().as_socket().unwrap();

        info!("Server: Started, listen: `{}`", &listen_addr);

        self.stop_flag.store(false, Ordering::Relaxed);
        self.listen_addr = Some(listen_addr.to_string());

        let fmt = self.fmt.clone();
        let listener: TcpListener = socket.try_clone().unwrap().into();
        let stop_flag = self.stop_flag.clone();
        let tx_map = self.tx_map.clone();
        let mut reader_handles = Vec::<JoinHandle<()>>::default();
        let mut writer_handles = Vec::<JoinHandle<()>>::default();
        self.handle = Some(std::thread::spawn(move || loop {
            if stop_flag.load(Ordering::Relaxed) {
                reader_handles.into_iter().for_each(|h| {
                    h.join().ok();
                });
                writer_handles.into_iter().for_each(|h| {
                    h.join().ok();
                });
                break;
            }

            match listener.accept() {
                Ok((stream, addr)) => {
                    info!("Server: Connection established, addr: `{}`", &addr);

                    {
                        let fmt = fmt.clone();
                        let mut stream = stream.try_clone().unwrap();
                        let stop_flag = stop_flag.clone();
                        reader_handles.push(std::thread::spawn(move || loop {
                            if stop_flag.load(Ordering::Relaxed) {
                                break;
                            }

                            match MessageDecoder::new(&fmt, &mut stream).decode(stop_flag.clone()) {
                                Ok(msg) => {
                                    info!("Server: Received from `{}`, msg: {:?}", addr, msg);
                                }
                                Err(Error::EndOfStream | Error::Stopped) => {
                                    break;
                                }
                                Err(e) => {
                                    warn!(
                                        "Server: Error occurs while reading message, error: {}",
                                        e
                                    );
                                }
                            }
                        }));
                    }

                    let (tx, rx) = channel::<Message>();

                    {
                        let fmt = fmt.clone();
                        let mut stream = stream.try_clone().unwrap();
                        writer_handles.push(std::thread::spawn(move || {
                            while let Ok(msg) = rx.recv() {
                                if let Ok(()) = MessageEncoder::new(&fmt, &mut stream).encode(&msg)
                                {
                                    info!("Server: Sent to `{}`, msg: {:?}", addr, msg);
                                } else {
                                    break;
                                }
                            }
                        }));
                    }

                    {
                        let mut tx_map = tx_map.lock().unwrap();

                        if stop_flag.load(Ordering::Relaxed) {
                            continue;
                        }

                        tx_map.insert(addr.to_string(), tx);
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    sleep(Duration::from_millis(500));
                }
                Err(e) => panic!("Encounter IO error: {:?}", e),
            }
        }));

        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(handle) = self.handle.take() {
            self.stop_flag.store(true, Ordering::Relaxed);
            self.listen_addr = None;
            {
                let mut tx_map = self.tx_map.lock().unwrap();
                tx_map.clear();
            }
            handle.join().unwrap();
        } else {
            panic!();
        }
    }

    pub fn send_msg(&mut self, addr: &str, msg: Message) -> Result<()> {
        let tx_map = self.tx_map.lock().unwrap();
        if let Some(tx) = tx_map.get(addr) {
            tx.send(msg).unwrap();
            Ok(())
        } else {
            Err(Error::NoSuchClient {
                addr: addr.to_string(),
            })
        }
    }
}

pub struct Client {
    fmt: MessageFormat,

    stop_flag: Arc<AtomicBool>,

    bind_addr: Option<String>,
    tx: Arc<Mutex<Option<Sender<Message>>>>,

    reader_handle: Option<JoinHandle<()>>,
    writer_handle: Option<JoinHandle<()>>,
}

impl Client {
    pub fn new(fmt: MessageFormat) -> Client {
        Client {
            fmt,
            stop_flag: Arc::new(AtomicBool::new(false)),
            bind_addr: None,
            tx: Default::default(),
            reader_handle: None,
            writer_handle: None,
        }
    }

    pub fn bind_addr(&self) -> &Option<String> {
        &self.bind_addr
    }

    pub fn run(&mut self, bind_addr: Option<&str>, connect_addr: &str) -> Result<()> {
        let connect_addr: SocketAddr = connect_addr.parse().map_err(|_| Error::AddrParse {
            invalid_addr: connect_addr.to_string(),
        })?;

        let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP)).unwrap();
        if let Some(bind_addr) = bind_addr {
            let bind_addr: SocketAddr = bind_addr.parse().map_err(|_| Error::AddrParse {
                invalid_addr: bind_addr.to_string(),
            })?;
            socket.bind(&bind_addr.into()).map_err(Error::Io)?;
        }
        socket
            .set_read_timeout(Some(Duration::from_millis(500)))
            .map_err(Error::Io)?;
        socket.connect(&connect_addr.into()).map_err(Error::Io)?;
        let bind_addr = socket.local_addr().map_err(Error::Io)?.as_socket().unwrap();

        info!(
            "Client: Started, bind: `{}`, connect to: `{}`",
            &bind_addr, &connect_addr
        );

        self.stop_flag.store(false, Ordering::Relaxed);
        self.bind_addr = Some(bind_addr.to_string());

        let fmt = self.fmt.clone();
        let stop_flag = self.stop_flag.clone();
        let mut stream: TcpStream = socket.try_clone().map_err(Error::Io)?.into();
        self.reader_handle = Some(std::thread::spawn(move || loop {
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }

            match MessageDecoder::new(&fmt, &mut stream).decode(stop_flag.clone()) {
                Ok(msg) => {
                    info!("Client: Received from `{}`, msg: {:?}", &connect_addr, &msg);
                }
                Err(Error::EndOfStream | Error::Stopped) => {
                    break;
                }
                Err(e) => {
                    warn!("Client: Error occurs while reading message, details: {}", e);
                }
            }
        }));

        let (tx, rx) = channel::<Message>();

        let fmt = self.fmt.clone();
        let mut stream: TcpStream = socket.try_clone().map_err(Error::Io)?.into();
        self.writer_handle = Some(std::thread::spawn(move || {
            while let Ok(msg) = rx.recv() {
                match MessageEncoder::new(&fmt, &mut stream).encode(&msg) {
                    Ok(()) => {
                        info!("Client: Sent to `{}`, msg: {:?}", &connect_addr, &msg);
                    }
                    Err(Error::Io(_)) => break,
                    Err(e) => warn!("Client: Failed to write message, error: {}", e),
                }
            }
        }));

        self.tx.lock().unwrap().replace(tx);

        Ok(())
    }

    pub fn stop(&mut self) {
        if let (Some(reader_handle), Some(writer_handle)) =
            (self.reader_handle.take(), self.writer_handle.take())
        {
            self.stop_flag.store(true, Ordering::Relaxed);
            self.bind_addr = None;
            {
                let mut tx = self.tx.lock().unwrap();
                tx.take();
            }
            reader_handle.join().unwrap();
            writer_handle.join().unwrap();
        } else {
            panic!();
        }
    }

    pub fn send_msg(&mut self, msg: Message) -> Result<()> {
        if let Some(tx) = self.tx.lock().unwrap().deref() {
            tx.send(msg).ok();
            Ok(())
        } else {
            Err(Error::NotConnected)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{thread::sleep, time::Duration};

    use simplelog::SimpleLogger;

    use crate::{
        msg::{ItemFormat, ItemValue, Message, MessageFormat},
        socket::Client,
    };

    use super::Server;

    #[test]
    fn send_msg_ok() {
        SimpleLogger::init(log::LevelFilter::Debug, Default::default()).unwrap();

        let fmt =
            MessageFormat::new(&[ItemFormat::Uint { len: 2 }, ItemFormat::Int { len: 1 }]).unwrap();

        let msg_client_1 = Message::new(vec![ItemValue::Uint(255), ItemValue::Int(7)]);
        let msg_client_2 = Message::new(vec![ItemValue::Uint(0), ItemValue::Int(-8)]);

        let msg_server_1 = Message::new(vec![ItemValue::Uint(255), ItemValue::Int(7)]);
        let msg_server_2 = Message::new(vec![ItemValue::Uint(0), ItemValue::Int(-8)]);

        let mut s = Server::new(fmt.clone());
        let mut c = Client::new(fmt);

        s.run(None).unwrap();
        let server_addr = s.listen_addr().as_ref().unwrap().clone();

        c.run(None, &server_addr).unwrap();
        let client_addr = c.bind_addr().as_ref().unwrap().clone();

        while s.client_len() == 0 {
            sleep(Duration::from_millis(500));
        }

        c.send_msg(msg_client_1).unwrap();
        c.send_msg(msg_client_2).unwrap();

        s.send_msg(&client_addr, msg_server_1).unwrap();
        s.send_msg(&client_addr, msg_server_2).unwrap();

        s.stop();
        c.stop();
    }
}
