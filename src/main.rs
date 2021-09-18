use eframe::{
    egui::{self, TextEdit, Widget},
    epi,
};
use hex::ToHex;
use log::warn;
use msg::{ItemFormat, ItemValue, Message, MessageFormat};
use simplelog::SimpleLogger;
use socket::{Client, Server};
use strum::IntoEnumIterator;

mod error;
mod msg;
mod socket;
mod ui;

#[derive(Debug, Clone, PartialEq, strum::ToString, strum::EnumIter)]
enum ItemKind {
    Len,
    Uint,
    Int,
    FixedString,
    VarString,
    FixedBytes,
    VarBytes,
}

impl ItemKind {
    fn from_item_format(fmt: &ItemFormat) -> Self {
        match fmt {
            ItemFormat::Len { len: _ } => Self::Len,
            ItemFormat::Uint { len: _ } => Self::Uint,
            ItemFormat::Int { len: _ } => Self::Int,
            ItemFormat::FixedString { len: _ } => Self::FixedString,
            ItemFormat::VarString { len_idx: _ } => Self::VarString,
            ItemFormat::FixedBytes { len: _ } => Self::FixedBytes,
            ItemFormat::VarBytes { len_idx: _ } => Self::VarBytes,
        }
    }

    fn get_default_item_format(&self) -> ItemFormat {
        match self {
            Self::Len => ItemFormat::Len { len: 0 },
            Self::Uint => ItemFormat::Uint { len: 1 },
            Self::Int => ItemFormat::Int { len: 1 },
            Self::FixedString => ItemFormat::FixedString { len: 1 },
            Self::VarString => ItemFormat::VarString { len_idx: 0 },
            Self::FixedBytes => ItemFormat::FixedBytes { len: 1 },
            Self::VarBytes => ItemFormat::VarBytes { len_idx: 0 },
        }
    }

    fn get_default_item_value(&self) -> ItemValue {
        match self {
            Self::Len => ItemValue::Len(0),
            Self::Uint => ItemValue::Uint(0),
            Self::Int => ItemValue::Int(0),
            Self::FixedString => ItemValue::String(Default::default()),
            Self::VarString => ItemValue::String(Default::default()),
            Self::FixedBytes => ItemValue::Bytes(Default::default()),
            Self::VarBytes => ItemValue::Bytes(Default::default()),
        }
    }
}

#[derive(Default)]
struct App {
    item_fmts: Vec<ItemFormat>,
    item_values: Vec<ItemValue>,

    decoded_msg: String,

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
            item_fmts,
            item_values,
            decoded_msg,
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
                ui.separator();

                // Format should not be modified after running.
                let can_modify_format = !*server_run_flag && !*client_run_flag;

                egui::Grid::new("message")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label("Format");
                        ui.label("Value");
                        ui.end_row();

                        let mut removed_idx = None;
                        for (idx, fmt) in item_fmts.iter_mut().enumerate() {
                            ui.vertical(|ui| {
                                ui.set_enabled(can_modify_format);

                                let mut kind = ItemKind::from_item_format(fmt);
                                let value = &mut item_values[idx];
                                egui::ComboBox::from_id_source(idx)
                                    .selected_text(kind.to_string())
                                    .show_ui(ui, |ui| {
                                        for k in ItemKind::iter() {
                                            ui.selectable_value(
                                                &mut kind,
                                                k.clone(),
                                                k.to_string(),
                                            );
                                        }
                                        if kind != ItemKind::from_item_format(fmt) {
                                            *fmt = kind.get_default_item_format();
                                            *value = kind.get_default_item_value();
                                        }
                                    });

                                match fmt {
                                    ItemFormat::Len { len } => {
                                        let mut len_str = len.to_string();
                                        ui.horizontal(|ui| {
                                            ui.label("Length:");
                                            ui.text_edit_singleline(&mut len_str);
                                        });
                                        *len = len_str.parse::<usize>().unwrap_or(1);
                                        *len = (*len).max(1);
                                    }
                                    ItemFormat::Uint { len }
                                    | ItemFormat::Int { len }
                                    | ItemFormat::FixedString { len }
                                    | ItemFormat::FixedBytes { len } => {
                                        let mut len_str = len.to_string();
                                        ui.horizontal(|ui| {
                                            ui.label("Length:");
                                            ui.text_edit_singleline(&mut len_str);
                                        });
                                        *len = len_str.parse::<usize>().unwrap_or(1);
                                        *len = (*len).max(1);
                                    }
                                    ItemFormat::VarString { len_idx }
                                    | ItemFormat::VarBytes { len_idx } => {
                                        let mut len_idx_str = len_idx.to_string();
                                        ui.horizontal(|ui| {
                                            ui.label("Length index:");
                                            ui.text_edit_singleline(&mut len_idx_str);
                                        });
                                        *len_idx = len_idx_str.parse::<usize>().unwrap_or(0);
                                    }
                                }
                            });

                            ui.vertical(|ui| {
                                if ui.button("Delete").clicked() {
                                    removed_idx = Some(idx);
                                }

                                let value = &mut item_values[idx];
                                match value {
                                    ItemValue::Len(v) => {
                                        ui.label(v.to_string());
                                    }
                                    ItemValue::Uint(v) => {
                                        let mut v_str = v.to_string();
                                        ui.text_edit_singleline(&mut v_str);
                                        *v = v_str.parse::<u64>().unwrap_or(0);
                                    }
                                    ItemValue::Int(v) => {
                                        let mut v_str = v.to_string();
                                        ui.text_edit_singleline(&mut v_str);
                                        *v = v_str.parse::<i64>().unwrap_or(0);
                                    }
                                    ItemValue::String(s) => {
                                        ui.text_edit_singleline(s);

                                        // Update the Len
                                        if let ItemFormat::VarString { len_idx } = fmt {
                                            let s_len = s.len() as u64;
                                            if let Some(ItemValue::Len(len)) =
                                                item_values.get_mut(*len_idx)
                                            {
                                                *len = s_len;
                                            }
                                        }
                                    }
                                    ItemValue::Bytes(bytes) => {
                                        let mut bytes_str: String = bytes.encode_hex_upper();
                                        ui.text_edit_singleline(&mut bytes_str);
                                        *bytes = hex::decode(bytes_str).unwrap_or_default();

                                        // Update the Len
                                        if let ItemFormat::VarBytes { len_idx } = fmt {
                                            let bytes_len = bytes.len() as u64;
                                            if let Some(ItemValue::Len(len)) =
                                                item_values.get_mut(*len_idx)
                                            {
                                                *len = bytes_len;
                                            }
                                        }
                                    }
                                };
                            });

                            ui.end_row();
                        }

                        if let Some(idx) = removed_idx {
                            item_fmts.remove(idx);
                            item_values.remove(idx);
                        }
                    });

                if egui::Button::new("Add message item")
                    .enabled(can_modify_format)
                    .ui(ui)
                    .clicked()
                {
                    item_fmts.push(ItemFormat::Len { len: 1 });
                    item_values.push(ItemValue::Len(0));
                }

                // Encoder and decoder.
                let msg_fmt = MessageFormat::new(item_fmts.clone());

                ui.horizontal(|ui| {
                    ui.label("Encode:");
                    ui.label(hex::encode_upper(
                        msg_fmt
                            .encode(&Message::new(item_values.clone()))
                            .unwrap_or_default(),
                    ));
                });

                ui.horizontal(|ui| {
                    ui.label("Decode:");
                    ui.text_edit_singleline(decoded_msg);
                    if ui.button("Confirm").clicked() {
                        match hex::decode(decoded_msg) {
                            Ok(bytes) => match msg_fmt.decode(&bytes) {
                                Ok(msg) => *item_values = msg.values().clone(),
                                Err(e) => warn!(
                                    "App: The bytes can not be decoded to Message, details: {}",
                                    e
                                ),
                            },
                            Err(e) => warn!(
                                "App: The string can not be decoded to bytes, details: {}",
                                e
                            ),
                        }
                    }
                });
            });

            // Group for server.
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Server");

                    if ui.add(ui::toggle(server_run_flag)).clicked() {
                        if *server_run_flag {
                            let mut new_server = Server::new(MessageFormat::new(item_fmts.clone()));

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
                });

                ui.separator();

                egui::Grid::new("server").num_columns(2).show(ui, |ui| {
                    ui.label("Connect count:");
                    ui.label(
                        server
                            .as_ref()
                            .map(|s| s.client_len().to_string())
                            .unwrap_or_default(),
                    );
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
                        .send_msg(server_target_addr, Message::new(item_values.clone()))
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

            // Group for client.
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Client");
                    if ui.add(ui::toggle(client_run_flag)).clicked() {
                        if *client_run_flag {
                            let mut new_client = Client::new(MessageFormat::new(item_fmts.clone()));

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
                                    warn!("App: Error occurs when run client, details: {}", e);
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
                });
                ui.separator();

                egui::Grid::new("client").num_columns(2).show(ui, |ui| {
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
                        .send_msg(Message::new(item_values.clone()))
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
    }
}

fn main() {
    SimpleLogger::init(log::LevelFilter::Info, Default::default()).unwrap();

    let app = App::default();
    eframe::run_native(Box::new(app), Default::default());
}
