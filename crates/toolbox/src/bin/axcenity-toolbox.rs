//! The `axcenity-toolbox` command line: see `axcenity_toolbox::cli`.

use std::time::Duration;

use axcenity_toolbox::{cli, directory, http};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let client = http::Client::new(Duration::from_secs(10 * 60));
    let (mut out, mut err) = (std::io::stdout().lock(), std::io::stderr().lock());
    let mut env = cli::Env {
        http: &client,
        default_dir: directory::locate(),
        out: &mut out,
        err: &mut err,
    };
    let code = cli::run(&args, &mut env);
    drop(env);
    std::process::exit(code);
}
