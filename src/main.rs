use simplelog::SimpleLogger;
use ui::app::App;

mod error;
mod msg;
mod socket;
mod ui;

fn main() {
    SimpleLogger::init(log::LevelFilter::Info, Default::default()).unwrap();

    let app = App::default();
    eframe::run_native(Box::new(app), Default::default());
}
