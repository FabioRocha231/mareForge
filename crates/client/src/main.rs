fn main() {
    if std::env::args().any(|arg| arg == "--playtest") {
        mareforge_client::playtest::prepare_playtest();
    }
    mareforge_client::windowed_app().run();
}
