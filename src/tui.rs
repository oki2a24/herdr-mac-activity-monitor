/// 3 秒ごとにメモリ・バッテリーを更新して表示する生 TUI（crossterm）。
/// `q` / `Esc` で終了。
use std::time::Duration;

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, KeyCode},
    execute,
    style::Print,
    terminal::{
        disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};

use crate::{battery, memory, parsers::MemInfo};

/// 生モード・別画面を有効にし、ループを走り、退出時にクリーンアップする。
pub fn run() -> std::io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, Hide)?;
    let result = run_loop(&mut stdout);
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
        render_frame(out, &crate::parsers::format_lines(&mem, &batt))?;
        // 入力待ち 3 秒。入力があれば読み取り、キーなら q/Esc で抜ける、
        // 別のイベントなら継続。タイムアウトなら次の 3 秒で再更新。
        if let Ok(true) = event::poll(tick) {
            if let event::Event::Key(k) = event::read()? {
                if matches!(k.code, KeyCode::Char('q') | KeyCode::Esc) {
                    break;
                }
            }
        }
    }
    Ok(())
}

/// 1フレームを描画する。描画前に画面をクリアしカーソルを原点(0,0)に戻してから、
/// 各行を上から順に出力する。これによりフレームごとに固定位置へ上書き描画され、
/// 下へ追記されて流れる現象を防ぐ。
fn render_frame<F: std::io::Write>(out: &mut F, lines: &[String]) -> std::io::Result<()> {
    execute!(out, Clear(ClearType::All), MoveTo(0, 0))?;
    for l in lines {
        execute!(out, Print(l), Print("\n"))?;
    }
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::render_frame;
    use crate::parsers::{format_lines, Battery, ChargingState, MemInfo};

    #[test]
    fn render_frame_clears_and_resets_to_origin_before_printing() {
        let lines = format_lines(
            &Some(MemInfo {
                used_gb: 1.0,
                total_gb: 2.0,
                percent: 50,
            }),
            &Battery::Present {
                percent: 83,
                state: ChargingState::Discharging,
            },
        );
        let mut buf: Vec<u8> = Vec::new();
        render_frame(&mut buf, &lines).unwrap();
        let s = String::from_utf8(buf).unwrap();

        let clear_all = "\x1b[2J";
        let move_origin = "\x1b[1;1H";

        assert!(
            s.starts_with(clear_all),
            "出力は画面クリアから始まるべき: {s:?}"
        );

        let clear_pos = s.find(clear_all).unwrap();
        let move_pos = s[clear_pos..].find(move_origin).unwrap();
        let move_abs = clear_pos + move_pos;
        let body_pos = s.find("Activity Monitor").unwrap();
        assert!(
            move_abs < body_pos,
            "本文より先に原点(0,0)へ移動すべき: {s:?}",
        );
        assert!(s.contains("83% discharging"), "本文が描画されるべき: {s:?}");
    }
}
