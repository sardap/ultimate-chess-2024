#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use bevy::{prelude::*, winit::WinitWindows};
use winit::window::Icon;

const ICO_BYTES: &[u8; 4030] = include_bytes!("../build/uc2024_icon.png");

fn set_window_icon(
    // we have to use `NonSend` here
    windows: NonSend<WinitWindows>,
) {
    // here we use the `image` crate to load our icon data from a png file
    // this is not a very bevy-native solution, but it will do
    let (icon_rgba, icon_width, icon_height) = {
        let image = image::load_from_memory_with_format(ICO_BYTES, image::ImageFormat::Png)
            .unwrap()
            .into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };
    let icon = Icon::from_rgba(icon_rgba, icon_width, icon_height).unwrap();

    // do it for all windows
    for window in windows.windows.values() {
        window.set_window_icon(Some(icon.clone()));
    }
}

fn main() {
    let mut app = bevy::prelude::App::new();
    uc2024::build_out_app(&mut app);
    app.add_systems(Startup, set_window_icon);
    app.run();
}
