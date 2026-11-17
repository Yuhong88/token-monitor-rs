use std::env;
use std::path::PathBuf;
use std::process::{exit, Command};

fn main() {
    let exe = env::current_exe().expect("current_exe");
    let dir = exe.parent().unwrap_or_else(|| std::path::Path::new("."));
    let llvm_dlltool: PathBuf = dir.join("llvm-dlltool.exe");

    let raw_args: Vec<String> = env::args().skip(1).collect();
    let mut filtered: Vec<String> = Vec::with_capacity(raw_args.len());
    let mut i = 0;
    while i < raw_args.len() {
        let arg = &raw_args[i];
        if arg == "-f" || arg == "--temp-prefix" {
            // Skip flag and its value
            i += 2;
            continue;
        }
        if arg.starts_with("--temp-prefix=") {
            i += 1;
            continue;
        }
        filtered.push(arg.clone());
        i += 1;
    }

    let status = Command::new(&llvm_dlltool)
        .args(&filtered)
        .status()
        .unwrap_or_else(|e| {
            eprintln!("failed to spawn {:?}: {}", llvm_dlltool, e);
            exit(1);
        });
    exit(status.code().unwrap_or(1));
}
