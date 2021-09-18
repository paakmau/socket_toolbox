use eframe::{
    egui::{self, TextEdit, Widget},
    epi,
};
use log::warn;
use strum::IntoEnumIterator;

use crate::{
    error::{Error, Result},
    msg::{ItemFormat, ItemValue, Message, MessageFormat},
    socket::{Client, Server},
};

use super::wrapper::ItemKindWrapper;
use super::{
    widget,
    wrapper::{ItemFormatWrapper, ItemValueWrapper},
};

#[derive(Default)]
pub struct App {
    item_kind_wrappers: Vec<ItemKindWrapper>,
    item_fmt_wrappers: Vec<ItemFormatWrapper>,
    item_value_wrappers: Vec<ItemValueWrapper>,

    item_parse_error: Option<Error>,

    item_fmts: Option<Vec<ItemFormat>>,
    item_values: Option<Vec<ItemValue>>,

    msg_fmt: Option<MessageFormat>,
    msg_fmt_validation_error: Option<Error>,

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
            item_kind_wrappers,
            item_fmt_wrappers,
            item_value_wrappers,
            item_parse_error,
            item_fmts,
            item_values,
            msg_fmt,
            msg_fmt_validation_error,
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
                        for (idx, (kind, fmt)) in item_kind_wrappers
                            .iter_mut()
                            .zip(item_fmt_wrappers.iter_mut())
                            .enumerate()
                        {
                            ui.vertical(|ui| {
                                ui.set_enabled(can_modify_format);

                                // ComboBox to select item kind.
                                let value = &mut item_value_wrappers[idx];
                                egui::ComboBox::from_id_source(idx)
                                    .selected_text(kind.to_string())
                                    .show_ui(ui, |ui| {
                                        for k in ItemKindWrapper::iter() {
                                            ui.selectable_value(kind, k.clone(), k.to_string());
                                        }
                                    });
                                // If kind changed, change format and value correspondingly.
                                if *kind != ItemKindWrapper::from_item_format(fmt) {
                                    *fmt = kind.default_item_format();
                                    *value = kind.default_item_value();
                                }

                                // Input item format.
                                match fmt {
                                    ItemFormatWrapper::Len { len }
                                    | ItemFormatWrapper::Uint { len }
                                    | ItemFormatWrapper::Int { len }
                                    | ItemFormatWrapper::FixedString { len }
                                    | ItemFormatWrapper::FixedBytes { len } => {
                                        ui.horizontal(|ui| {
                                            ui.label("Length:");
                                            ui.text_edit_singleline(len);
                                        });
                                    }
                                    ItemFormatWrapper::VarString { len_idx }
                                    | ItemFormatWrapper::VarBytes { len_idx } => {
                                        ui.horizontal(|ui| {
                                            ui.label("Length index:");
                                            ui.text_edit_singleline(len_idx);
                                        });
                                    }
                                }
                            });

                            // Input item value.
                            ui.vertical(|ui| {
                                if ui.button("Delete").clicked() {
                                    removed_idx = Some(idx);
                                }

                                let value = &mut item_value_wrappers[idx];
                                match value {
                                    ItemValueWrapper::Len(v) => {
                                        ui.label(v.to_string());
                                    }
                                    ItemValueWrapper::Uint(s)
                                    | ItemValueWrapper::Int(s)
                                    | ItemValueWrapper::Bytes(s)
                                    | ItemValueWrapper::String(s) => {
                                        ui.text_edit_singleline(s);
                                    }
                                };

                                // Update the Len
                                match (fmt, value) {
                                    (
                                        ItemFormatWrapper::VarString { len_idx },
                                        ItemValueWrapper::String(s),
                                    ) => {
                                        if let Ok(len_idx) = len_idx.parse::<usize>() {
                                            let s_len = s.len() as u64;
                                            if let Some(ItemValueWrapper::Len(len)) =
                                                item_value_wrappers.get_mut(len_idx)
                                            {
                                                *len = s_len;
                                            }
                                        }
                                    }
                                    (
                                        ItemFormatWrapper::VarBytes { len_idx },
                                        ItemValueWrapper::Bytes(s),
                                    ) => {
                                        if let Ok(len_idx) = len_idx.parse::<usize>() {
                                            let s_len = s.len() as u64 >> 1;
                                            if let Some(ItemValueWrapper::Len(len)) =
                                                item_value_wrappers.get_mut(len_idx)
                                            {
                                                *len = s_len;
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            });

                            ui.end_row();
                        }

                        if let Some(idx) = removed_idx {
                            item_fmt_wrappers.remove(idx);
                            item_value_wrappers.remove(idx);
                        }
                    });

                *item_parse_error = None;
                *item_fmts = None;
                *item_values = None;

                // Parse item formats.
                match item_fmt_wrappers
                    .iter()
                    .enumerate()
                    .map(|(idx, fmt)| fmt.parse().map_err(|e| e.global_error(idx)))
                    .collect::<Result<Vec<ItemFormat>>>()
                {
                    Ok(fmts) => *item_fmts = Some(fmts),
                    Err(e) => *item_parse_error = Some(e),
                }

                // Parse item values.
                match item_value_wrappers
                    .iter()
                    .enumerate()
                    .map(|(idx, value)| value.parse().map_err(|e| e.global_error(idx)))
                    .collect::<Result<Vec<ItemValue>>>()
                {
                    Ok(values) => *item_values = Some(values),
                    Err(e) => *item_parse_error = Some(e),
                }

                if egui::Button::new("Add message item")
                    .enabled(can_modify_format)
                    .ui(ui)
                    .clicked()
                {
                    item_kind_wrappers.push(ItemKindWrapper::Len);
                    item_fmt_wrappers
                        .push(item_kind_wrappers.last().unwrap().default_item_format());
                    item_value_wrappers
                        .push(item_kind_wrappers.last().unwrap().default_item_value());
                }

                // Construct message format.
                if let Some(item_fmts) = item_fmts {
                    let fmt = MessageFormat::new(item_fmts.clone());
                    *msg_fmt_validation_error = fmt.err();
                    if msg_fmt_validation_error.is_some() {
                        *msg_fmt = None;
                    } else {
                        *msg_fmt = Some(fmt);
                    }
                }

                ui.separator();

                // Show parse error if exists.
                if let Some(e) = item_parse_error.as_ref() {
                    ui.label(format!("Parse error: {}", e));
                }

                // Show validation error if exists.
                if let Some(e) = msg_fmt_validation_error {
                    ui.label(format!("validation error: {}", e));
                } else {
                    let msg_fmt = msg_fmt.as_ref().unwrap();

                    if let Some(item_values) = item_values.as_ref() {
                        // Encode the input to bytes, show errors if fails.
                        let res = msg_fmt.encode(&Message::new(item_values.clone()));
                        match res {
                            Ok(bytes) => {
                                ui.label(format!("Encode: {}", hex::encode_upper(bytes)));
                            }
                            Err(e) => {
                                ui.label(format!("Encode error: {}", e));
                            }
                        }
                    }

                    // Decode the bytes to input, log errors if fails.
                    ui.horizontal(|ui| {
                        ui.label("Decode:");
                        ui.text_edit_singleline(decoded_msg);
                        if ui.button("Confirm").clicked() {
                            match hex::decode(decoded_msg) {
                                Ok(bytes) => match msg_fmt.decode(&bytes) {
                                    Ok(msg) => {
                                        *item_value_wrappers = msg
                                            .values()
                                            .iter()
                                            .map(ItemValueWrapper::from)
                                            .collect()
                                    }
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
                }
            });

            // Group for server.
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Server");

                    if widget::toggle(server_run_flag).ui(ui).clicked() {
                        if *server_run_flag {
                            let mut new_server = Server::new(MessageFormat::new(
                                item_fmts.as_ref().unwrap().clone(),
                            ));

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
                        .send_msg(
                            server_target_addr,
                            Message::new(item_values.as_ref().unwrap().clone()),
                        )
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
                    // Client shouldn't run if item formats is not valid.
                    if !item_fmts.is_some() {
                        ui.set_enabled(false);
                    }

                    ui.label("Client");
                    if widget::toggle(client_run_flag).ui(ui).clicked() {
                        if *client_run_flag {
                            let mut new_client = Client::new(MessageFormat::new(
                                item_fmts.as_ref().unwrap().clone(),
                            ));

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
                        .send_msg(Message::new(item_values.as_ref().unwrap().clone()))
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
