use clap::Parser;

#[derive(Parser, Debug)]
pub struct Args {
    #[arg(long, default_value = "codex")]
    pub codecli_bin: String,

    #[arg(trailing_var_arg = true)]
    pub codecli_args: Vec<String>,

    #[arg(long, default_value_t = 65536)]
    pub capture_bytes: usize,
}
