use eframe::{
    egui::{self, TextEdit, Widget},
    epi,
};
use hex::ToHex;
use log::warn;
use msg::{DataFormat, DataValue, Message, MessageFormat};
use simplelog::SimpleLogger;
use socket::{Client, Server};
use strum::IntoEnumIterator;

mod error;
mod msg;
mod socket;
mod ui;

#[derive(Debug, Clone, PartialEq, strum::ToString, strum::EnumIter)]
enum DataKind {
    Len,
    Uint,
    Int,
    FixedString,
    VarString,
    FixedBytes,
    VarBytes,
}

impl DataKind {
    fn from_data_format(fmt: &DataFormat) -> Self {
        match fmt {
            DataFormat::Len {
                len: _,
                data_idx: _,
            } => Self::Len,
            DataFormat::Uint { len: _ } => Self::Uint,
            DataFormat::Int { len: _ } => Self::Int,
            DataFormat::FixedString { len: _ } => Self::FixedString,
            DataFormat::VarString { len_idx: _ } => Self::VarString,
            DataFormat::FixedBytes { len: _ } => Self::FixedBytes,
            DataFormat::VarBytes { len_idx: _ } => Self::VarBytes,
        }
    }

    fn get_default_data_format(&self) -> DataFormat {
        match self {
            DataKind::Len => DataFormat::Len {
                len: 0,
                data_idx: 0,
            },
            DataKind::Uint => DataFormat::Uint { len: 0 },
            DataKind::Int => DataFormat::Int { len: 0 },
            DataKind::FixedString => DataFormat::FixedString { len: 0 },
            DataKind::VarString => DataFormat::VarString { len_idx: 0 },
            DataKind::FixedBytes => DataFormat::FixedBytes { len: 0 },
            DataKind::VarBytes => DataFormat::VarBytes { len_idx: 0 },
        }
    }

    fn get_default_data_value(&self) -> DataValue {
        match self {
            DataKind::Len => DataValue::Len(0),
            DataKind::Uint => DataValue::Uint(0),
            DataKind::Int => DataValue::Int(0),
            DataKind::FixedString => DataValue::String(Default::default()),
            DataKind::VarString => DataValue::String(Default::default()),
            DataKind::FixedBytes => DataValue::Bytes(Default::default()),
            DataKind::VarBytes => DataValue::Bytes(Default::default()),
        }
    }
}

#[derive(Default)]
struct App {
    data_fmts: Vec<DataFormat>,
    data_values: Vec<DataValue>,

    client_bind_addr: String,
    client_connect_addr: String,
    client_run_flag: bool,
    client: Option<Client>,

    server_listen_addr: String,
    server_run_flag: bool,
    server: Option<Server>,
    server_target_addr: String,
}

impl epi::App for App {
    fn name(&self) -> &str {
        "Socket Toolbox"
    }

    fn setup(
        &mut self,
        _ctx: &eframe::egui::CtxRef,
        _frame: &mut epi::Frame<'_>,
        _storage: Option<&dyn epi::Storage>,
    ) {
    }

    fn save(&mut self, _storage: &mut dyn epi::Storage) {}

    fn update(&mut self, ctx: &eframe::egui::CtxRef, _frame: &mut epi::Frame<'_>) {
        let Self {
            data_fmts,
            data_values,
            client_bind_addr,
            client_connect_addr,
            client_run_flag,
            client,
            server_listen_addr,
            server_run_flag,
            server,
            server_target_addr,
        } = self;

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.group(|ui| {
                ui.label("Message");

                // Format should not be modified after running.
                let can_modify_format = !*server_run_flag && !*client_run_flag;

                egui::Grid::new("message")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label("Format");
                        ui.label("Value");
                        ui.end_row();

                        for (idx, (fmt, value)) in
                            data_fmts.iter_mut().zip(data_values.iter_mut()).enumerate()
                        {
                            ui.vertical(|ui| {
                                ui.set_enabled(can_modify_format);

                                let mut kind = DataKind::from_data_format(fmt);
                                egui::ComboBox::from_id_source(idx)
                                    .selected_text(kind.to_string())
                                    .show_ui(ui, |ui| {
                                        for k in DataKind::iter() {
                                            ui.selectable_value(
                                                &mut kind,
                                                k.clone(),
                                                k.to_string(),
                                            );
                                        }
                                        if kind != DataKind::from_data_format(fmt) {
                                            *fmt = kind.get_default_data_format();
                                            *value = kind.get_default_data_value();
                                        }
                                    });

                                match fmt {
                                    DataFormat::Len { len, data_idx } => {
                                        let mut len_str = len.to_string();
                                        ui.text_edit_singleline(&mut len_str);
                                        *len = len_str.parse::<usize>().unwrap_or(1);
                                        *len = (*len).max(1);

                                        let mut data_idx_str = data_idx.to_string();
                                        ui.text_edit_singleline(&mut data_idx_str);
                                        *data_idx = data_idx_str.parse::<usize>().unwrap_or(0);
                                    }
                                    DataFormat::Uint { len }
                                    | DataFormat::Int { len }
                                    | DataFormat::FixedString { len }
                                    | DataFormat::FixedBytes { len } => {
                                        let mut len_str = len.to_string();
                                        ui.text_edit_singleline(&mut len_str);
                                        *len = len_str.parse::<usize>().unwrap_or(1);
                                        *len = (*len).max(1);
                                    }
                                    DataFormat::VarString { len_idx }
                                    | DataFormat::VarBytes { len_idx } => {
                                        let mut len_idx_str = len_idx.to_string();
                                        ui.text_edit_singleline(&mut len_idx_str);
                                        *len_idx = len_idx_str.parse::<usize>().unwrap_or(0);
                                    }
                                }
                            });

                            ui.vertical(|ui| match value {
                                DataValue::Len(v) | DataValue::Uint(v) => {
                                    let mut v_str = v.to_string();
                                    ui.text_edit_singleline(&mut v_str);
                                    *v = v_str.parse::<u64>().unwrap_or(0);
                                }
                                DataValue::Int(v) => {
                                    let mut v_str = v.to_string();
                                    ui.text_edit_singleline(&mut v_str);
                                    *v = v_str.parse::<i64>().unwrap_or(0);
                                }
                                DataValue::String(s) => {
                                    ui.text_edit_singleline(s);
                                }
                                DataValue::Bytes(bytes) => {
                                    let mut bytes_str: String = bytes.encode_hex_upper();
                                    ui.text_edit_singleline(&mut bytes_str);
                                    *bytes = hex::decode(bytes_str).unwrap_or(Default::default());
                                }
                            });

                            ui.end_row();
                        }
                    });

                if egui::Button::new("Add message item")
                    .enabled(can_modify_format)
                    .ui(ui)
                    .clicked()
                {
                    data_fmts.push(DataFormat::Len {
                        len: 1,
                        data_idx: 0,
                    });
                    data_values.push(DataValue::Len(0));
                }
            });

            ui.horizontal(|ui| {
                // Group for server.
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        egui::Grid::new("server").num_columns(2).show(ui, |ui| {
                            ui.label("server");
                            if ui.add(ui::toggle(server_run_flag)).clicked() {
                                if *server_run_flag {
                                    let mut new_server =
                                        Server::new(MessageFormat::new(data_fmts.clone()));

                                    let listen_addr = if server_listen_addr.is_empty() {
                                        None
                                    } else {
                                        Some(server_listen_addr.as_str())
                                    };

                                    new_server.run(listen_addr).err().iter().for_each(|e| {
                                        warn!("App: Error occurs when run server, details: {}", e);
                                        *server_run_flag = false;
                                    });

                                    if *server_run_flag {
                                        *server_listen_addr =
                                            new_server.listen_addr().as_ref().unwrap().clone();
                                        server.replace(new_server);
                                    }
                                } else {
                                    server.take().unwrap().stop();
                                }
                            }
                            ui.end_row();

                            ui.label("Listen:");
                            // Server listen address should not be modified while running.
                            TextEdit::singleline(server_listen_addr)
                                .enabled(!*server_run_flag)
                                .ui(ui);
                            ui.end_row();

                            ui.label("Send to:");
                            ui.text_edit_singleline(server_target_addr);
                        });

                        if ui
                            .add(egui::Button::new("send message").enabled(*server_run_flag))
                            .clicked()
                        {
                            server
                                .as_mut()
                                .unwrap()
                                .send_msg(server_target_addr, Message::new(data_values.clone()))
                                .err()
                                .iter()
                                .for_each(|e| {
                                    warn!(
                                "App: Error occurs when send message to client `{}`, details: {}",
                                server_target_addr, e
                            );
                                });
                        }
                    });
                });

                // Group for client.
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        egui::Grid::new("client").num_columns(2).show(ui, |ui| {
                            ui.label("client");
                            if ui.add(ui::toggle(client_run_flag)).clicked() {
                                if *client_run_flag {
                                    let mut new_client =
                                        Client::new(MessageFormat::new(data_fmts.clone()));

                                    let bind_addr = if client_bind_addr.is_empty() {
                                        None
                                    } else {
                                        Some(client_bind_addr.as_str())
                                    };

                                    new_client
                                        .run(bind_addr, client_connect_addr)
                                        .err()
                                        .iter()
                                        .for_each(|e| {
                                            warn!(
                                                "App: Error occurs when run client, details: {}",
                                                e
                                            );
                                            *client_run_flag = false;
                                        });

                                    if *client_run_flag {
                                        *client_bind_addr =
                                            new_client.bind_addr().as_ref().unwrap().clone();

                                        client.replace(new_client);
                                    }
                                } else {
                                    client.take().unwrap().stop();
                                }
                            }
                            ui.end_row();

                            ui.label("Bind:");
                            // Client bind address should not be modified while running.
                            TextEdit::singleline(client_bind_addr)
                                .enabled(!*client_run_flag)
                                .ui(ui);
                            ui.end_row();

                            ui.label("Connect to:");
                            // Client listen address should not be modified while running.
                            TextEdit::singleline(client_connect_addr)
                                .enabled(!*client_run_flag)
                                .ui(ui);
                            ui.end_row();
                        });

                        if ui
                            .add(egui::Button::new("send message").enabled(*client_run_flag))
                            .clicked()
                        {
                            client
                                .as_mut()
                                .unwrap()
                                .send_msg(Message::new(data_values.clone()))
                                .err()
                                .iter()
                                .for_each(|e| {
                                    warn!(
                                    "App: Error occurs when send message to server, details: {}",
                                    e
                                );
                                });
                        }
                    });
                });
            });
        });
    }
}

fn main() {
    SimpleLogger::init(log::LevelFilter::Info, Default::default()).unwrap();

    let app = App::default();
    eframe::run_native(Box::new(app), Default::default());
}
