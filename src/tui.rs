/// 3 秒ごとにメモリ・バッテリーを更新して表示する生 TUI（crossterm）。
/// `q` / `Esc` で終了。
use std::time::Duration;

use crossterm::{
    cursor::{Hide, Show},
    event::{self, KeyCode},
    execute,
    style::Print,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::{battery, memory, parsers::MemInfo};

/// 生モード・別画面を有効にし、ループを走り、退出時にクリーンアップする。
pub fn run() -> std::io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, Hide)?;
    let result = run_loop(&mut stdout);
    crate::herdr::clear_window_title().ok();
    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen, Show)?;
    result
}

/// 3 秒ごと（またはキー入力時）にデータを取得して redraw する。
/// `q` / `Esc` でループから抜ける。
fn run_loop<F: std::io::Write>(out: &mut F) -> std::io::Result<()> {
    let tick = Duration::from_secs(3);
    loop {
        let mem: Option<MemInfo> = memory::get().ok();
        let batt = battery::get();
        for l in crate::parsers::format_lines(&mem, &batt) {
            execute!(out, Print(l), Print("\n"))?;
        }
        out.flush().ok();
        let window_title = crate::parsers::format_window_title(&mem, &batt);
        if let Err(e) = crate::herdr::set_window_title(&window_title) {
            eprintln!("warn: failed to set window title: {e}");
        }
        // 入力待ち 3 秒。入力があれば読み取り、キーなら q/Esc で抜ける、
        // 別のイベントなら継続。タイムアウトなら次の 3 秒で再更新。
        match event::poll(tick) {
            Ok(true) => {
                if let event::Event::Key(k) = event::read()? {
                    match k.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        _ => {}
                    }
                }
            }
            Ok(false) | Err(_) => {}
        }
    }
    Ok(())
}
