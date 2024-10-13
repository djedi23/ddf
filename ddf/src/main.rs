mod args;
mod filesystem;
mod fsext;
mod settings;
mod trace;

use crate::{
  args::{gen_completions, App},
  filesystem::Filesystem,
  settings::Exclusion::{FsType, MountDirStartsWith},
};
use anyhow::Result;
use clap::{CommandFactory, Parser};
use fsext::read_fs_list;
use humansize::{make_format, FormatSizeOptions, BINARY};
use ratatui::{prelude::Backend, Terminal, Viewport};
use settings::{settings, Settings};
use trace::init_tracing;
use tracing::{debug, trace};

fn main() -> Result<()> {
  init_tracing()?;
  let args = App::parse();
  let config = settings(&App::command().get_matches())?;
  gen_completions(&args);

  debug!("{:#?}", args);

  let mounts = read_fs_list()?;
  let filesystems: Vec<Filesystem> = if let Some(files) = args.files {
    files
      .iter()
      .filter_map(|file| Filesystem::from_path(&mounts, file))
      .collect()
  } else {
    mounts
      .into_iter()
      .filter_map(|m| Filesystem::new(m, None))
      .filter(|fs| fs.usage.blocks > 0)
      .filter(|fs| {
        !config.exclude.as_ref().unwrap_or(&vec![]).iter().any(
          |exclusion_rule| match exclusion_rule {
            MountDirStartsWith(name) => fs.mount_info.mount_dir.starts_with(name),
            FsType(typ) => fs.mount_info.fs_type == *typ,
          },
        )
      })
      .collect()
  };

  debug!("{filesystems:#?}");
  let column_config = filesystems
    .iter()
    .map(|f| (f.mount_info.dev_name.len(), f.mount_info.mount_dir.len()))
    .reduce(|acc, e| (acc.0.max(e.0), acc.1.max(e.1)))
    .unwrap_or((10, 10));

  trace!("{column_config:?}");

  render_table(filesystems, config, column_config)?;
  Ok(())
}

fn render_table(
  filesystems: Vec<Filesystem>,
  config: Settings,
  columns_width: (usize, usize),
) -> Result<(), anyhow::Error> {
  let mut terminal = ratatui::init_with_options(ratatui::TerminalOptions {
    viewport: Viewport::Inline(1),
  });
  for filesystem in filesystems {
    render_line(&filesystem, &mut terminal, &config, columns_width)?;
  }
  ratatui::restore();
  Ok(())
}

fn render_line<A: Backend>(
  fs: &Filesystem,
  terminal: &mut Terminal<A>,
  settings: &Settings,
  columns_width: (usize, usize),
) -> Result<()> {
  use ratatui::{prelude::*, widgets::*};
  terminal.insert_before(1, |frame| {
    let [a_fs, a_size, a_used, a_avail, a_dir, a_percent] = Layout::default()
      .direction(Direction::Horizontal)
      .constraints([
        Constraint::Length(columns_width.0 as u16 + 1),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(columns_width.1 as u16 + 1),
        Constraint::Fill(1),
      ])
      .areas(*frame.area());

    let bused = fs.usage.blocks.saturating_sub(fs.usage.bfree);
    let percent_used = bused as f64 / (bused + fs.usage.bavail) as f64;

    let formatter = make_format(
      FormatSizeOptions::from(BINARY)
        .space_after_value(false)
        .decimal_places(1),
    );

    Paragraph::new(fs.mount_info.dev_name.clone()).render(a_fs, frame);
    Paragraph::new(format!(
      "{:>9}",
      formatter(fs.usage.blocks * fs.usage.blocksize,)
    ))
    .render(a_size, frame);
    Paragraph::new(format!("{:>9}", formatter(bused * fs.usage.blocksize,))).render(a_used, frame);
    Paragraph::new(format!(
      "{:>9}",
      formatter(fs.usage.bavail * fs.usage.blocksize,)
    ))
    .render(a_avail, frame);
    Paragraph::new(fs.mount_info.mount_dir.clone()).render(a_dir, frame);
    LineGauge::default()
      .filled_style(
        Style::default()
          .fg(if percent_used > settings.high_threshold() {
            Color::Red
          } else if percent_used > settings.medium_threshold() {
            Color::Yellow
          } else {
            Color::Green
          })
          .add_modifier(Modifier::BOLD),
      )
      .line_set(symbols::line::DOUBLE)
      .unfilled_style(Style::default().fg(Color::DarkGray))
      .label(format!("{:>3}%", (100.0 * percent_used).round()))
      .ratio(percent_used)
      .render(a_percent, frame);
  })?;
  Ok(())
}
