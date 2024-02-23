fn main() {
    let mut app = bevy::prelude::App::new();
    uc2024::build_out_app(&mut app);
    app.run();
}
