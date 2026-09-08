use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    DefaultTerminal, Frame,
};
use similar::{ChangeTag, TextDiff};
use tui_textarea::TextArea;

const ACCENT: Color = Color::Cyan;
const MUTED: Color = Color::DarkGray;
const DEL: Color = Color::Red;
const ADD: Color = Color::Green;

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
                (true, KeyCode::Char('k')) => {
                    if self.mode == Mode::Chunk {
                        self.editor = make_editor(&self.chunks[self.idx]);
                        self.commit_and_next();
                    }
                }
                (true, KeyCode::Char('o')) => {
                    if self.mode == Mode::Chunk {
                        self.editor = make_editor(&self.chunks[self.idx]);
                    }
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
        let [title_a, body_a, foot_a] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(f.area());

        self.draw_chunk_title(f, title_a);

        let original = self.chunks[self.idx].clone();
        let diff = diff_lines(&original, &self.editor_text());

        let content_width = body_a.width.saturating_sub(2);
        let diff_len: usize = diff
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.chars().count())
            .sum();
        let orig_h = wrapped_row_count(&original, content_width) + 2;
        let diff_h = row_count_for_len(diff_len, content_width) + 2;
        let [orig_a, diff_a, edit_a] = split_panes(body_a, orig_h, diff_h);

        let orig_para = Paragraph::new(original.as_str())
            .wrap(Wrap { trim: false })
            .block(panel_block("Original (AI)", false));
        let diff_para = Paragraph::new(diff)
            .wrap(Wrap { trim: false })
            .block(panel_block("Diff", false));

        f.render_widget(orig_para, orig_a);
        f.render_widget(diff_para, diff_a);

        self.editor.set_block(panel_block("Your version", true));
        f.render_widget(&self.editor, edit_a);

        f.render_widget(
            Paragraph::new(hints(&[
                ("Ctrl+S", "commit + next"),
                ("Ctrl+K", "keep original"),
                ("Ctrl+O", "copy original in"),
                ("Ctrl+P", "back"),
                ("Ctrl+Q", "quit"),
            ])),
            foot_a,
        );
    }

    fn draw_chunk_title(&self, f: &mut Frame, area: Rect) {
        let counter = format!(" {}/{} ", self.idx + 1, self.chunks.len());
        let bar_width: u16 = 20;
        let counter_width = counter.len() as u16;
        let [name_a, bar_a, counter_a] = Layout::horizontal([
            Constraint::Min(0),
            Constraint::Length(bar_width),
            Constraint::Length(counter_width),
        ])
        .areas(area);

        f.render_widget(
            Paragraph::new(format!(" {}", self.input_name))
                .style(Style::default().add_modifier(Modifier::BOLD)),
            name_a,
        );
        f.render_widget(
            Paragraph::new(progress_bar(self.idx + 1, self.chunks.len(), bar_width)),
            bar_a,
        );
        f.render_widget(
            Paragraph::new(counter).style(Style::default().fg(MUTED)),
            counter_a,
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
            Paragraph::new(Line::from(vec![
                Span::styled(
                    " Final preview ",
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("· saving to {}", self.output.display()),
                    Style::default().fg(MUTED),
                ),
            ])),
            title_a,
        );

        self.editor.set_block(panel_block("Full document (last minute edits here)", true));
        f.render_widget(&self.editor, edit_a);

        f.render_widget(
            Paragraph::new(hints(&[
                ("Ctrl+S", "save and exit"),
                ("Ctrl+P", "back to chunks (discards preview edits)"),
                ("Ctrl+Q", "quit"),
            ])),
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

fn panel_block(title: &str, focused: bool) -> Block<'static> {
    let color = if focused { ACCENT } else { MUTED };
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(format!(" {title} "))
        .border_style(Style::default().fg(color))
}

/// Splits the chunk-view body into original/diff/editor areas. The original
/// and diff panes are sized to their wrapped content (so a one-line paragraph
/// doesn't reserve a third of the screen), capped at a share of the
/// available height so the editor always keeps most of the room.
fn split_panes(body: Rect, orig_h: u16, diff_h: u16) -> [Rect; 3] {
    let avail = body.height;
    let min_pane = 3u16;
    let min_editor = 5u16;

    let max_orig = ((avail as f32) * 0.42).round() as u16;
    let max_diff = ((avail as f32) * 0.30).round() as u16;

    let mut orig_h = orig_h.clamp(min_pane, max_orig.max(min_pane));
    let mut diff_h = diff_h.clamp(min_pane, max_diff.max(min_pane));

    let total_min = orig_h + diff_h + min_editor;
    if total_min > avail {
        let mut overflow = total_min - avail;

        let diff_shrink = overflow.min(diff_h.saturating_sub(min_pane));
        diff_h -= diff_shrink;
        overflow -= diff_shrink;

        let orig_shrink = overflow.min(orig_h.saturating_sub(min_pane));
        orig_h -= orig_shrink;
    }

    let edit_h = avail.saturating_sub(orig_h + diff_h);

    Layout::vertical([
        Constraint::Length(orig_h),
        Constraint::Length(diff_h),
        Constraint::Length(edit_h),
    ])
    .areas(body)
}

/// Approximate greedy-wrapped row count for `text` at the given content
/// width. Only used to size panes to their content, so exactness doesn't
/// matter as much as staying in the right ballpark.
fn wrapped_row_count(text: &str, width: u16) -> u16 {
    text.split('\n')
        .map(|line| row_count_for_len(line.chars().count(), width))
        .sum::<u16>()
        .max(1)
}

fn row_count_for_len(len: usize, width: u16) -> u16 {
    let width = width.max(1) as usize;
    if len == 0 {
        1
    } else {
        len.div_ceil(width) as u16
    }
}

fn progress_bar(current: usize, total: usize, width: u16) -> Line<'static> {
    let width = width.max(1) as usize;
    let filled = if total == 0 {
        0
    } else {
        ((current as f64 / total as f64) * width as f64).round() as usize
    }
    .min(width);

    let mut spans = Vec::new();
    if filled > 0 {
        spans.push(Span::styled(
            "━".repeat(filled),
            Style::default().fg(ACCENT),
        ));
    }
    if width > filled {
        spans.push(Span::styled(
            "━".repeat(width - filled),
            Style::default().fg(MUTED),
        ));
    }
    Line::from(spans)
}

/// Renders a row of key hints as `Key description  ·  Key description`.
fn hints(pairs: &[(&str, &str)]) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    for (i, (key, desc)) in pairs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ·  ", Style::default().fg(MUTED)));
        }
        spans.push(Span::styled(
            key.to_string(),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(format!(" {desc}"), Style::default().fg(MUTED)));
    }
    Line::from(spans)
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
            ChangeTag::Delete => Style::default().fg(DEL).add_modifier(Modifier::CROSSED_OUT),
            ChangeTag::Insert => Style::default().fg(ADD),
            ChangeTag::Equal => Style::default().fg(MUTED),
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
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(DEL)),
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
