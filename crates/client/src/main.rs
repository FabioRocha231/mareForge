fn main() {
    if std::env::args().any(|arg| arg == "--playtest") {
        marvyr_client::playtest::prepare_playtest();
    }
    marvyr_client::windowed_app().run();
}
