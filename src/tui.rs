//! 3 秒ごとにメモリ・バッテリーを更新して表示する生 TUI（crossterm）。
//! `q` / `Esc` で終了。
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

use crate::{memory, metrics::PopupState};

/// 生モード・別画面を有効にし、ループを走り、退出時にクリーンアップする。
pub fn run() -> std::io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, Hide)?;
    let result = run_loop(&mut stdout, event::poll, event::read, || memory::get().ok());
    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen, Show)?;
    result
}

/// 3秒ごと、またはキー入力時にデータを取得して画面を再描画する。
/// `q` / `Esc` でループから抜ける。
fn run_loop<F, P, R, M>(
    out: &mut F,
    mut poll: P,
    mut read: R,
    mut get_memory: M,
) -> std::io::Result<()>
where
    F: std::io::Write,
    P: FnMut(Duration) -> std::io::Result<bool>,
    R: FnMut() -> std::io::Result<event::Event>,
    M: FnMut() -> Option<crate::parsers::MemInfo>,
{
    let tick = Duration::from_secs(3);
    let mut popup = PopupState::new();
    loop {
        let mem = get_memory();
        let snapshot = popup.collect(mem);
        render_frame(out, &crate::metrics::format_popup_lines(&snapshot))?;
        // 入力待ち 3 秒。入力があれば読み取り、キーなら q/Esc で抜ける、
        // 別のイベントなら継続。タイムアウトなら次の 3 秒で再更新。
        if let Ok(true) = poll(tick) {
            if let event::Event::Key(k) = read()? {
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
        // raw mode では LF だけでは列が戻らないため、CR も出力する。
        execute!(out, Print(l), Print("\r\n"))?;
    }
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::{render_frame, run_loop};
    use crate::parsers::{format_lines, Battery, ChargingState, MemInfo};
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    use std::time::Duration;

    #[test]
    /// 通常キーとタイムアウトでは再描画を続け、qでループを終了する。
    // why: 入力がない時間帯や終了以外のキーで監視画面が止まらないため。
    fn run_loop_redraws_after_timeout_or_other_key_and_exits_on_q() {
        let mut output = Vec::new();
        let mut polls = [true, false, true].into_iter();
        let mut poll_count = 0;
        let mut keys = [
            Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)),
            Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
        ]
        .into_iter();
        let mut read_count = 0;
        let mut memory_count = 0;

        run_loop(
            &mut output,
            |timeout| {
                assert_eq!(timeout, Duration::from_secs(3));
                poll_count += 1;
                Ok(polls.next().unwrap())
            },
            || {
                read_count += 1;
                Ok(keys.next().unwrap())
            },
            || {
                memory_count += 1;
                Some(MemInfo {
                    used_gb: 1.0,
                    total_gb: 2.0,
                    percent: 50,
                })
            },
        )
        .unwrap();

        let output = String::from_utf8(output).unwrap();
        assert_eq!(poll_count, 3);
        assert_eq!(read_count, 2);
        assert_eq!(memory_count, 3);
        assert_eq!(output.matches("\x1b[2J").count(), 3);
    }

    #[test]
    /// Escapeキーで監視画面のループを終了する。
    // why: q以外にも説明されている終了操作を使えるようにするため。
    fn run_loop_exits_on_escape() {
        let mut output = Vec::new();
        let mut polls = 0;

        run_loop(
            &mut output,
            |_| {
                polls += 1;
                Ok(true)
            },
            || Ok(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))),
            || {
                Some(MemInfo {
                    used_gb: 1.0,
                    total_gb: 2.0,
                    percent: 50,
                })
            },
        )
        .unwrap();

        assert_eq!(polls, 1);
        assert_eq!(
            String::from_utf8(output)
                .unwrap()
                .matches("\x1b[2J")
                .count(),
            1
        );
    }

    #[test]
    /// 端末の生モードで各行の先頭が左端に揃うようCRLFを出力する。
    // why: 端末の生モードでも各行の開始位置を揃えるため。
    fn render_frame_aligns_rows_to_left_edge_in_raw_mode() {
        let lines = format_lines(&None, &Battery::Absent);
        let mut buf = Vec::new();
        render_frame(&mut buf, &lines).unwrap();
        let output = String::from_utf8(buf).unwrap();
        let body = output.strip_prefix("\x1b[2J\x1b[1;1H").unwrap();

        // raw mode では LF は行だけを進め、CR が列を左端へ戻す。
        let (mut row, mut column) = (0, 0);
        let mut starts = Vec::new();
        for ch in body.chars() {
            match ch {
                '\r' => column = 0,
                '\n' => row += 1,
                _ => {
                    if starts.len() == row {
                        starts.push((row, column));
                    }
                    column += 1;
                }
            }
        }
        assert_eq!(starts, vec![(0, 0), (1, 0), (2, 0)]);
    }

    #[test]
    /// 描画前に画面全体を消去し、カーソルを原点へ戻す。
    // why: 再描画時に古い文字や行ずれを残さないため。
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
