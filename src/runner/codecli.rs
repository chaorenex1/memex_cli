use std::process::Stdio;

use tokio::process::{Child, Command};

use crate::cli::Args;
use crate::error::RunnerError;

pub fn spawn(args: &Args) -> Result<Child, RunnerError> {
    let mut cmd = Command::new(&args.codecli_bin);
    cmd.args(&args.codecli_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    cmd.spawn().map_err(|e| RunnerError::Spawn(e.to_string()))
}
