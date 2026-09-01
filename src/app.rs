use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    DefaultTerminal, Frame,
};
use similar::{ChangeTag, TextDiff};
use tui_textarea::TextArea;

#[derive(PartialEq)]
enum Mode {
    Chunk,
    Preview,
}

pub struct App {
    input_name: String,
    output: PathBuf,
    chunks: Vec<String>,
    drafts: Vec<String>,
    idx: usize,
    editor: TextArea<'static>,
    mode: Mode,
    confirm_quit: bool,
}

impl App {
    pub fn new(input: &Path, output: PathBuf, chunks: Vec<String>) -> Self {
        let drafts = vec![String::new(); chunks.len()];
        Self {
            input_name: input.display().to_string(),
            output,
            chunks,
            drafts,
            idx: 0,
            editor: TextArea::default(),
            mode: Mode::Chunk,
            confirm_quit: false,
        }
    }

    /// Returns Some(output path) if the document was saved, None if aborted.
    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<Option<PathBuf>> {
        self.load_chunk_editor();
        loop {
            terminal.draw(|f| self.draw(f))?;

            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }

            if self.confirm_quit {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => return Ok(None),
                    _ => self.confirm_quit = false,
                }
                continue;
            }

            let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
            match (ctrl, key.code) {
                (true, KeyCode::Char('q')) => self.confirm_quit = true,
                (true, KeyCode::Char('s')) => match self.mode {
                    Mode::Chunk => self.commit_and_next(),
                    Mode::Preview => {
                        self.save()?;
                        return Ok(Some(self.output.clone()));
                    }
                },
                (true, KeyCode::Char('k')) if self.mode == Mode::Chunk => {
                    self.editor = make_editor(&self.chunks[self.idx].clone());
                    self.commit_and_next();
                }
                (true, KeyCode::Char('o')) if self.mode == Mode::Chunk => {
                    self.editor = make_editor(&self.chunks[self.idx].clone());
                }
                (true, KeyCode::Char('p')) => self.go_back(),
                _ => {
                    self.editor.input(key);
                }
            }
        }
    }

    fn editor_text(&self) -> String {
        self.editor.lines().join("\n")
    }

    fn load_chunk_editor(&mut self) {
        self.editor = make_editor(&self.drafts[self.idx]);
    }

    fn commit_and_next(&mut self) {
        self.drafts[self.idx] = self.editor_text();
        if self.idx + 1 < self.chunks.len() {
            self.idx += 1;
            self.load_chunk_editor();
        } else {
            self.enter_preview();
        }
    }

    fn enter_preview(&mut self) {
        let doc = self
            .drafts
            .iter()
            .filter(|d| !d.trim().is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n");
        self.editor = make_editor(&doc);
        self.mode = Mode::Preview;
    }

    fn go_back(&mut self) {
        match self.mode {
            Mode::Preview => {
                self.mode = Mode::Chunk;
                self.idx = self.chunks.len() - 1;
                self.load_chunk_editor();
            }
            Mode::Chunk if self.idx > 0 => {
                self.drafts[self.idx] = self.editor_text();
                self.idx -= 1;
                self.load_chunk_editor();
            }
            Mode::Chunk => {}
        }
    }

    fn save(&self) -> Result<()> {
        let mut doc = self.editor_text();
        if !doc.ends_with('\n') {
            doc.push('\n');
        }
        std::fs::write(&self.output, doc)
            .with_context(|| format!("cannot write {}", self.output.display()))
    }

    fn draw(&mut self, f: &mut Frame) {
        match self.mode {
            Mode::Chunk => self.draw_chunk(f),
            Mode::Preview => self.draw_preview(f),
        }
        if self.confirm_quit {
            draw_confirm_quit(f);
        }
    }

    fn draw_chunk(&mut self, f: &mut Frame) {
        let [title_a, orig_a, diff_a, edit_a, foot_a] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Percentage(30),
            Constraint::Percentage(22),
            Constraint::Min(6),
            Constraint::Length(1),
        ])
        .areas(f.area());

        let title = format!(
            " {} | chunk {}/{} ",
            self.input_name,
            self.idx + 1,
            self.chunks.len()
        );
        f.render_widget(
            Paragraph::new(title).style(Style::default().add_modifier(Modifier::BOLD)),
            title_a,
        );

        let original = &self.chunks[self.idx];
        f.render_widget(
            Paragraph::new(original.as_str())
                .wrap(Wrap { trim: false })
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Original (AI) ")
                        .border_style(Style::default().fg(Color::DarkGray)),
                ),
            orig_a,
        );

        let diff = diff_lines(original, &self.editor_text());
        f.render_widget(
            Paragraph::new(diff)
                .wrap(Wrap { trim: false })
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Diff (original vs yours) ")
                        .border_style(Style::default().fg(Color::DarkGray)),
                ),
            diff_a,
        );

        self.editor.set_block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Your version ")
                .border_style(Style::default().fg(Color::Cyan)),
        );
        f.render_widget(&self.editor, edit_a);

        f.render_widget(
            Paragraph::new(
                " Ctrl+S commit + next | Ctrl+K keep original | Ctrl+O copy original in | Ctrl+P back | Ctrl+Q quit ",
            )
            .style(Style::default().fg(Color::DarkGray)),
            foot_a,
        );
    }

    fn draw_preview(&mut self, f: &mut Frame) {
        let [title_a, edit_a, foot_a] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(6),
            Constraint::Length(1),
        ])
        .areas(f.area());

        f.render_widget(
            Paragraph::new(format!(
                " Final preview | saving to {} ",
                self.output.display()
            ))
            .style(Style::default().add_modifier(Modifier::BOLD)),
            title_a,
        );

        self.editor.set_block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Full document (last minute edits here) ")
                .border_style(Style::default().fg(Color::Green)),
        );
        f.render_widget(&self.editor, edit_a);

        f.render_widget(
            Paragraph::new(
                " Ctrl+S save and exit | Ctrl+P back to chunks (discards preview edits) | Ctrl+Q quit ",
            )
            .style(Style::default().fg(Color::DarkGray)),
            foot_a,
        );
    }
}

fn make_editor(content: &str) -> TextArea<'static> {
    let lines: Vec<String> = if content.is_empty() {
        Vec::new()
    } else {
        content.lines().map(String::from).collect()
    };
    let mut editor = TextArea::new(lines);
    editor.set_cursor_line_style(Style::default());
    editor.set_placeholder_text("Write your own version here...");
    editor
}

/// Word-level diff rendered as one wrapped line. Whitespace is normalised
/// to single spaces here; this pane is a visualisation, the real text
/// lives in the original and editor panes.
fn diff_lines(original: &str, draft: &str) -> Vec<Line<'static>> {
    let old: Vec<&str> = original.split_whitespace().collect();
    let new: Vec<&str> = draft.split_whitespace().collect();
    let diff = TextDiff::from_slices(&old, &new);
    let mut spans = Vec::new();
    for change in diff.iter_all_changes() {
        let style = match change.tag() {
            ChangeTag::Delete => Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::CROSSED_OUT),
            ChangeTag::Insert => Style::default().fg(Color::Green),
            ChangeTag::Equal => Style::default().fg(Color::DarkGray),
        };
        spans.push(Span::styled(format!("{} ", change.value()), style));
    }
    vec![Line::from(spans)]
}

fn draw_confirm_quit(f: &mut Frame) {
    let area = centered_rect(40, 5, f.area());
    f.render_widget(Clear, area);
    f.render_widget(
        Paragraph::new("\nQuit without saving? (y/n)")
            .centered()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Red)),
            ),
        area,
    );
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}
