use std::process::{Child, Command};
use std::time::Duration;

use mareforge_client::playtest::{disable_dev_automation, PLAYTEST_BANNER};

const SERVER_ARG: &str = "--server";

fn main() {
    if std::env::args().any(|arg| arg == SERVER_ARG) {
        mareforge_server::run_headless();
        return;
    }

    disable_dev_automation();
    println!("{PLAYTEST_BANNER}");
    let _server = ServerProcess::spawn();
    // ponytail: fixed 250ms wait; poll server readiness if slow machines appear.
    std::thread::sleep(Duration::from_millis(250));
    mareforge_client::windowed_app().run();
}

struct ServerProcess(Child);

impl ServerProcess {
    fn spawn() -> Self {
        let exe = std::env::current_exe().expect("current playtest binary");
        let child = Command::new(exe)
            .arg(SERVER_ARG)
            .arg("--playtest")
            .spawn()
            .expect("start playtest server child");
        Self(child)
    }
}

impl Drop for ServerProcess {
    fn drop(&mut self) {
        // SIGTERM (não SIGKILL) para que o child `--playtest` grave o resumo
        // JSON no encerramento ordenado antes de sair.
        unsafe {
            libc::kill(self.0.id() as libc::pid_t, libc::SIGTERM);
        }
        let _ = self.0.wait();
    }
}
