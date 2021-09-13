use eframe::{egui, epi};
use hex::ToHex;
use msg::{DataFormat, DataValue};
use strum::IntoEnumIterator;

mod msg;
mod socket;

#[derive(Debug, Clone, PartialEq, strum::ToString, strum::EnumIter)]
enum DataKind {
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
        } = self;

        egui::CentralPanel::default().show(ctx, |ui| {
            for (idx, (fmt, value)) in data_fmts.iter_mut().zip(data_values.iter_mut()).enumerate()
            {
                let mut kind = DataKind::from_data_format(fmt);
                egui::ComboBox::from_id_source(idx)
                    .selected_text(kind.to_string())
                    .show_ui(ui, |ui| {
                        for k in DataKind::iter() {
                            ui.selectable_value(&mut kind, k.clone(), k.to_string());
                        }
                        if kind != DataKind::from_data_format(fmt) {
                            *fmt = kind.get_default_data_format();
                            *value = kind.get_default_data_value();
                        }
                    });

                match fmt {
                    DataFormat::Uint { len }
                    | DataFormat::Int { len }
                    | DataFormat::FixedString { len }
                    | DataFormat::FixedBytes { len } => {
                        let mut len_str = len.to_string();
                        ui.text_edit_singleline(&mut len_str);
                        *len = len_str.parse::<usize>().unwrap_or(1);
                        *len = (*len).max(1);
                    }
                    DataFormat::VarString { len_idx } | DataFormat::VarBytes { len_idx } => {
                        let mut len_idx_str = len_idx.to_string();
                        ui.text_edit_singleline(&mut len_idx_str);
                        *len_idx = len_idx_str.parse::<usize>().unwrap_or(0);
                    }
                }

                match value {
                    DataValue::Uint(v) => {
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
                }
            }
            if ui.button("Add item").clicked() {
                data_fmts.push(DataFormat::Uint { len: 1 });
                data_values.push(DataValue::Uint(0));
            }
        });
    }
}

fn main() {
    let app = App::default();
    eframe::run_native(Box::new(app), Default::default());
}
