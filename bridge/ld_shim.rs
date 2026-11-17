use std::env;
use std::process::{exit, Command};

fn main() {
    let exe = env::current_exe().expect("current_exe");
    let sc_dir = exe.parent().unwrap_or_else(|| std::path::Path::new("."));
    let bin_dir = sc_dir.parent().unwrap_or(sc_dir);
    let ld_lld = bin_dir.join("gcc-ld").join("ld.lld.exe");

    let status = Command::new(&ld_lld)
        .args(env::args().skip(1))
        .status()
        .unwrap_or_else(|e| {
            eprintln!("failed to spawn {:?}: {}", ld_lld, e);
            exit(1);
        });
    exit(status.code().unwrap_or(1));
}
