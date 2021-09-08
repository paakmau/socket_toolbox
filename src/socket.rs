use std::{
    collections::HashMap,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{channel, Sender},
        Arc, Mutex,
    },
    thread::{sleep, JoinHandle},
    time::Duration,
};

use log::info;
use socket2::{Domain, Protocol, Socket, Type};

use crate::msg::{Message, MessageFormat};

pub struct Server {
    fmt: MessageFormat,

    stop_flag: Arc<AtomicBool>,

    tx_map: Arc<Mutex<HashMap<String, Sender<Message>>>>,

    handle: Option<JoinHandle<()>>,
}

impl Server {
    pub fn new(fmt: MessageFormat) -> Self {
        Self {
            fmt,
            stop_flag: Arc::new(AtomicBool::new(false)),
            tx_map: Default::default(),
            handle: None,
        }
    }

    pub fn run(&mut self, listen_addr: String) -> Result<(), ()> {
        let listen_addr: SocketAddr = listen_addr.parse().map_err(|_| ())?;

        let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP)).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_millis(1500)))
            .map_err(|_| ())?;
        socket
            .set_write_timeout(Some(Duration::from_millis(1500)))
            .map_err(|_| ())?;
        socket.bind(&listen_addr.into()).map_err(|_| ())?;
        socket.listen(2).unwrap();

        info!("Server: Started, listen: {}", &listen_addr);

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
                    info!("Server: Connection established, addr: {}", &addr);

                    {
                        let fmt = fmt.clone();
                        let mut stream = stream.try_clone().unwrap();
                        let stop_flag = stop_flag.clone();
                        reader_handles.push(std::thread::spawn(move || loop {
                            if stop_flag.load(Ordering::Relaxed) {
                                break;
                            }

                            if let Ok(msg) = fmt.read_from(&mut stream) {
                                info!("Server: Received from {}, msg: {:?}", addr, msg);
                            } else {
                                break;
                            }
                        }));
                    }

                    let (tx, rx) = channel::<Message>();

                    {
                        let fmt = fmt.clone();
                        let mut stream = stream.try_clone().unwrap();
                        writer_handles.push(std::thread::spawn(move || loop {
                            if let Ok(msg) = rx.recv() {
                                if let Ok(()) = fmt.write_to(&msg, &mut stream) {
                                    info!("Server: Sent to {}, msg: {:?}", addr, msg);
                                } else {
                                    break;
                                }
                            } else {
                                break;
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
                    sleep(Duration::from_millis(1500));
                }
                Err(e) => panic!("Encounter IO error: {:?}", e),
            }
        }));

        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), ()> {
        if let Some(handle) = self.handle.take() {
            self.stop_flag.store(true, Ordering::Relaxed);
            {
                let mut tx_map = self.tx_map.lock().unwrap();
                tx_map.clear();
            }
            handle.join().map_err(|_| ())?;
            Ok(())
        } else {
            Err(())
        }
    }

    pub fn send_msg(&mut self, addr: String, msg: Message) -> Result<(), ()> {
        let tx_map = self.tx_map.lock().unwrap();
        if let Some(tx) = tx_map.get(&addr) {
            tx.send(msg).unwrap();
            Ok(())
        } else {
            Err(())
        }
    }
}

pub struct Client {
    fmt: MessageFormat,

    stop_flag: Arc<AtomicBool>,

    tx: Arc<Mutex<Option<Sender<Message>>>>,

    reader_handle: Option<JoinHandle<()>>,
    writer_handle: Option<JoinHandle<()>>,
}

impl Client {
    pub fn new(fmt: MessageFormat) -> Client {
        Client {
            fmt,
            stop_flag: Arc::new(AtomicBool::new(false)),
            tx: Default::default(),
            reader_handle: None,
            writer_handle: None,
        }
    }

    pub fn run(&mut self, bind_addr: String, connect_addr: String) -> Result<(), ()> {
        let bind_addr: SocketAddr = bind_addr.parse().map_err(|_| ())?;
        let connect_addr: SocketAddr = connect_addr.parse().map_err(|_| ())?;

        let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP)).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_millis(1500)))
            .map_err(|_| ())?;
        socket
            .set_write_timeout(Some(Duration::from_millis(1500)))
            .map_err(|_| ())?;
        socket.bind(&bind_addr.into()).map_err(|_| ())?;
        socket.connect(&connect_addr.into()).map_err(|_| ())?;

        info!(
            "Client: Started, bind: {}, connect to: {}",
            &bind_addr, &connect_addr
        );

        let fmt = self.fmt.clone();
        let stop_flag = self.stop_flag.clone();
        let mut stream: TcpStream = socket.try_clone().map_err(|_| ())?.into();
        self.reader_handle = Some(std::thread::spawn(move || loop {
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }

            if let Ok(msg) = fmt.read_from(&mut stream) {
                info!("Client: Received from {}, msg: {:?}", &connect_addr, &msg);
            } else {
                break;
            }
        }));

        let (tx, rx) = channel::<Message>();

        let fmt = self.fmt.clone();
        let mut stream: TcpStream = socket.try_clone().map_err(|_| ())?.into();
        self.writer_handle = Some(std::thread::spawn(move || loop {
            if let Ok(msg) = rx.recv() {
                if let Ok(()) = fmt.write_to(&msg, &mut stream) {
                    info!("Client: Sent to {}, msg: {:?}", &connect_addr, &msg);
                } else {
                    break;
                }
            } else {
                break;
            }
        }));

        self.tx.lock().unwrap().replace(tx);

        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), ()> {
        if let (Some(reader_handle), Some(writer_handle)) =
            (self.reader_handle.take(), self.writer_handle.take())
        {
            {
                let mut tx = self.tx.lock().unwrap();
                tx.take();
            }
            self.stop_flag.store(true, Ordering::Relaxed);
            reader_handle.join().map_err(|_| ())?;
            writer_handle.join().map_err(|_| ())?;
            Ok(())
        } else {
            Err(())
        }
    }

    pub fn send_msg(&mut self, msg: Message) -> Result<(), ()> {
        let tx = self.tx.lock().unwrap().take().unwrap();
        tx.send(msg).unwrap();
        Ok(())
    }
}
