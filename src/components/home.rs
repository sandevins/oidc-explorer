use std::cmp::min;

use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind, MouseButton};
use crossterm::terminal::size as terminal_size;
use ratatui::prelude::*;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use tokio::sync::mpsc::UnboundedSender;

use super::Component;
use crate::{action::Action, config::Config};

/// Focusable widgets in the login card.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Focus {
    Username,
    Password,
    LoginButton,
    ChangeServer,
}

impl Focus {
    fn next(self) -> Self {
        match self {
            Focus::Username => Focus::Password,
            Focus::Password => Focus::LoginButton,
            Focus::LoginButton => Focus::ChangeServer,
            Focus::ChangeServer => Focus::Username,
        }
    }

    fn prev(self) -> Self {
        match self {
            Focus::Username => Focus::ChangeServer,
            Focus::Password => Focus::Username,
            Focus::LoginButton => Focus::Password,
            Focus::ChangeServer => Focus::LoginButton,
        }
    }
}

pub struct Home {
    command_tx: Option<UnboundedSender<Action>>,
    config: Config,

    // UI state
    username: String,
    password: String,
    focus: Focus,
    cursor_pos: usize,
    status: String,
}

impl Home {
    pub fn new() -> Self {
        Self {
            command_tx: None,
            config: Config::new().unwrap_or_default(),
            username: String::new(),
            password: String::new(),
            focus: Focus::Username,
            cursor_pos: 0,
            status: String::new(),
        }
    }

    fn set_status(&mut self, s: impl Into<String>) {
        self.status = s.into();
    }

    // Insert a char at the cursor for the focused input
    fn insert_char(&mut self, ch: char) {
        match self.focus {
            Focus::Username => {
                let pos = min(self.cursor_pos, self.username.len());
                self.username.insert(pos, ch);
                self.cursor_pos = pos + ch.len_utf8();
            }
            Focus::Password => {
                let pos = min(self.cursor_pos, self.password.len());
                self.password.insert(pos, ch);
                self.cursor_pos = pos + ch.len_utf8();
            }
            _ => {}
        }
    }

    // Delete a char before the cursor (backspace)
    fn backspace(&mut self) {
        match self.focus {
            Focus::Username => {
                if self.cursor_pos > 0 && !self.username.is_empty() {
                    let new_pos = self.username[..self.cursor_pos].chars().rev().next().map(|c| c.len_utf8()).unwrap_or(1);
                    let cut = self.cursor_pos - new_pos;
                    self.username.replace_range(cut..self.cursor_pos, "");
                    self.cursor_pos = cut;
                }
            }
            Focus::Password => {
                if self.cursor_pos > 0 && !self.password.is_empty() {
                    let new_pos = self.password[..self.cursor_pos].chars().rev().next().map(|c| c.len_utf8()).unwrap_or(1);
                    let cut = self.cursor_pos - new_pos;
                    self.password.replace_range(cut..self.cursor_pos, "");
                    self.cursor_pos = cut;
                }
            }
            _ => {}
        }
    }

    fn delete(&mut self) {
        match self.focus {
            Focus::Username => {
                if self.cursor_pos < self.username.len() {
                    let next_len = self.username[self.cursor_pos..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                    self.username.replace_range(self.cursor_pos..self.cursor_pos + next_len, "");
                }
            }
            Focus::Password => {
                if self.cursor_pos < self.password.len() {
                    let next_len = self.password[self.cursor_pos..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                    self.password.replace_range(self.cursor_pos..self.cursor_pos + next_len, "");
                }
            }
            _ => {}
        }
    }

    fn move_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos = self.username[..self.cursor_pos]
                .chars()
                .rev()
                .next()
                .map(|c| self.cursor_pos - c.len_utf8())
                .unwrap_or(0);
        }
    }

    fn move_right(&mut self) {
        let len = match self.focus {
            Focus::Username => self.username.len(),
            Focus::Password => self.password.len(),
            _ => 0,
        };
        if self.cursor_pos < len {
            let next = match self.focus {
                Focus::Username => self.username[self.cursor_pos..].chars().next().unwrap(),
                Focus::Password => self.password[self.cursor_pos..].chars().next().unwrap(),
                _ => '\0',
            };
            self.cursor_pos += next.len_utf8();
        }
    }
}

impl Component for Home {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.command_tx = Some(tx);
        Ok(())
    }

    fn register_config_handler(&mut self, config: Config) -> Result<()> {
        self.config = config;
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        // Map keys to behavior (tab, shift-tab, enter, escape, character input, arrows, backspace)
        match key.code {
            KeyCode::Tab => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.focus = self.focus.prev();
                } else {
                    self.focus = self.focus.next();
                }
            }
            KeyCode::Char('q') | KeyCode::Esc => {
                return Ok(Some(Action::Quit));
            }
            KeyCode::Left => self.move_left(),
            KeyCode::Right => self.move_right(),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Enter | KeyCode::Char(' ') => {
                match self.focus {
                    Focus::LoginButton => {
                        // For now, just set status and log
                        tracing::info!(username = %self.username, password = %self.password, "Login pressed");
                        self.set_status(format!("Logging in as '{}' (password {} chars)", self.username, self.password.len()));
                    }
                    Focus::ChangeServer => {
                        tracing::info!("Change server pressed");
                        self.set_status("Change server pressed (TODO: implement)".to_string());
                    }
                    _ => {}
                }
            }
            KeyCode::Char(ch) => {
                // Regular char input: append to focused field
                if !key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.insert_char(ch);
                }
            }
            _ => {}
        }
        Ok(None)
    }

    fn handle_mouse_event(&mut self, mouse: MouseEvent) -> Result<Option<Action>> {
        // Very basic hit-testing: when clicked inside certain Y ranges, focus corresponding widget.
        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            let x = mouse.column as u16;
            let y = mouse.row as u16;
            // We'll emit a special action to trigger a redraw and store focus.
            // The draw function computes geometry; here we approximate by using the terminal size via config.
            // To keep things simple and robust, clicking anywhere will cycle focus based on Y position.
            // A production version should translate coordinates precisely.
            // Heuristic mapping (top to bottom): username approx top third, password next, login btn next, change server last.
            let (_w, height) = terminal_size().unwrap_or((80, 24));
            let card_top = height / 6;
            let card_height = height / 2;
            let rel_y = if y > card_top { y - card_top } else { 0 };
            let third = card_height / 6;
            if rel_y <= third * 4 {
                self.focus = Focus::Username;
                // set cursor position roughly
                self.cursor_pos = min(rel_y as usize, self.username.len());
            } else if rel_y <= third * 5 {
                self.focus = Focus::Password;
                self.cursor_pos = min(rel_y as usize, self.password.len());
            } else if rel_y <= third * 6 {
                self.focus = Focus::LoginButton;
                // Activate on click
                tracing::info!(username = %self.username, password_len = self.password.len(), "Login pressed (mouse)");
                self.set_status(format!("Logging in as '{}' (password {} chars)", self.username, self.password.len()));
            } else {
                self.focus = Focus::ChangeServer;
                tracing::info!("Change server pressed (mouse)");
                self.set_status("Change server pressed (TODO: implement)".to_string());
            }
        }
        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Tick => {}
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        // Overall dark background
        let _bg = Style::new().bg(Color::Black).fg(Color::White);
        frame.render_widget(Clear, area);

        // Centered card: use Layout to create a centered area
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(15),
                Constraint::Percentage(70),
                Constraint::Percentage(15),
            ])
            .split(area);

        let card_area = chunks[1];
        let card = Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .style(Style::new().bg(Color::Rgb(20, 20, 20)).fg(Color::White));

        frame.render_widget(card, card_area);

        // Inner layout within card
        let inner = Layout::default()
            .direction(Direction::Vertical)
            .margin(2)
            .constraints([
                Constraint::Length(11), // logo
                Constraint::Length(1), // title
                Constraint::Length(3), // username
                Constraint::Length(3), // password
                Constraint::Length(3), // login button
                Constraint::Length(3), // change server
                Constraint::Min(1),    // status
            ])
            .split(card_area);

        let logo_block = Block::default()
            .borders(Borders::NONE)
            .style(Style::new().bg(Color::Rgb(30, 30, 30)));
            let logo = r"           /$$       /$$                                                /$$                                        
          |__/      | $$                                               | $$                                        
  /$$$$$$  /$$  /$$$$$$$  /$$$$$$$         /$$$$$$  /$$   /$$  /$$$$$$ | $$  /$$$$$$   /$$$$$$   /$$$$$$   /$$$$$$ 
 /$$__  $$| $$ /$$__  $$ /$$_____//$$$$$$ /$$__  $$|  $$ /$$/ /$$__  $$| $$ /$$__  $$ /$$__  $$ /$$__  $$ /$$__  $$
| $$  \ $$| $$| $$  | $$| $$     |______/| $$$$$$$$ \  $$$$/ | $$  \ $$| $$| $$  \ $$| $$  \__/| $$$$$$$$| $$  \__/
| $$  | $$| $$| $$  | $$| $$             | $$_____/  >$$  $$ | $$  | $$| $$| $$  | $$| $$      | $$_____/| $$      
|  $$$$$$/| $$|  $$$$$$$|  $$$$$$$       |  $$$$$$$ /$$/\  $$| $$$$$$$/| $$|  $$$$$$/| $$      |  $$$$$$$| $$      
 \______/ |__/ \_______/ \_______/        \_______/|__/  \__/| $$____/ |__/ \______/ |__/       \_______/|__/      
                                                             | $$                                                  
                                                             | $$                                                  
                                                             |__/                                                  ";
            let logo_para = Paragraph::new(logo).style(Style::new().fg(Color::Red)).alignment(Alignment::Center);
        frame.render_widget(logo_block.clone(), inner[0]);
        frame.render_widget(logo_para, inner[0]);

        // Input field rendering helper
        let render_input = |frame: &mut Frame, area: Rect, label: &str, value: &str, focused: bool, cursor_pos: usize, mask: bool| {
            let label_area = Rect::new(area.x, area.y, 12, area.height);
            let input_area = Rect::new(area.x + 13, area.y, area.width.saturating_sub(13), area.height);

            let label_par = Paragraph::new(label).style(Style::new().fg(Color::Gray));
            frame.render_widget(label_par, label_area);

            let display = if mask { "*".repeat(value.chars().count()) } else { value.to_string() };
            let mut input = Paragraph::new(display.clone()).style(
                if focused {
                    Style::new().fg(Color::Black).bg(Color::White)
                } else {
                    Style::new().fg(Color::White).bg(Color::Rgb(30, 30, 30))
                },
            );
            input = input.block(Block::default().borders(Borders::ALL).border_type(ratatui::widgets::BorderType::Rounded).style(
                if focused { Style::new().fg(Color::Yellow) } else { Style::new().fg(Color::Gray) }
            ));
            frame.render_widget(input, input_area);

            // When focused, set the cursor
            if focused {
                let cx = input_area.x + 1 + cursor_pos as u16; // naive: treat chars as single width
                let cy = input_area.y + input_area.height / 2;
                frame.set_cursor(cx, cy);
            }
        };

        // Username field
        render_input(
            frame,
            inner[2],
            "username",
            &self.username,
            self.focus == Focus::Username,
            self.cursor_pos,
            false,
        );

        // Password field
        render_input(
            frame,
            inner[3],
            "password",
            &self.password,
            self.focus == Focus::Password,
            self.cursor_pos,
            true,
        );

        // Button helper
        let render_button = |frame: &mut Frame, area: Rect, label: &str, focused: bool| {
            let btn = Paragraph::new(label).style(
                if focused { Style::new().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD) }
                else { Style::new().fg(Color::White).bg(Color::Rgb(30, 30, 30)) }
            ).alignment(Alignment::Center);
            let btn_area = Rect::new(area.x + area.width / 4, area.y, area.width / 2, area.height);
            frame.render_widget(btn, btn_area);
        };

        render_button(frame, inner[4], "Login", self.focus == Focus::LoginButton);
        render_button(frame, inner[5], "Change server", self.focus == Focus::ChangeServer);

        // Status line
        let status = Paragraph::new(self.status.clone()).style(Style::new().fg(Color::LightBlue)).wrap(Wrap { trim: true });
        frame.render_widget(status, inner[6]);

        Ok(())
    }
}
