use clap::Parser;
use clap_complete::Shell;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub(crate) struct App {
  /// List of file systems or mount points to display (optional).
  pub(crate) files: Option<Vec<String>>,
  #[arg(long, value_enum)]
  completion: Option<Shell>,
}

pub(crate) fn gen_completions(args: &App) {
  if let Some(generator) = args.completion {
    use clap::{Command, CommandFactory};
    use clap_complete::{generate, Generator};
    use std::io;
    fn print_completions<G: Generator>(gen: G, cmd: &mut Command) {
      generate(gen, cmd, cmd.get_name().to_string(), &mut io::stdout());
    }
    let mut cmd = App::command();
    eprintln!("Generating completion file for {generator:?}...");
    print_completions(generator, &mut cmd);
    std::process::exit(0);
  }
}
