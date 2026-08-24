mod agent;
mod app;
mod config;
mod ui;
mod workspace;

use relm4::RelmApp;

use crate::app::App;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("commando=info".parse().expect("directive")),
        )
        .with_target(false)
        .init();

    let app = RelmApp::new("app.commando.Commando");
    relm4::set_global_css(include_str!("style.css"));
    app.run::<App>(());
}
