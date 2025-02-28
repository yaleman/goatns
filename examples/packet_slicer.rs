use std::io;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Stylize,
    symbols::border,
    text::{Line, Text},
    widgets::{Block, Paragraph, Widget},
    DefaultTerminal, Frame,
};

// struct Packet {
//     src: IpAddr,
//     dest: IpAddr,
//     src_port: u32,
//     dest_port: u32,
//     data: Vec<u8>,
// }

#[derive(Default, Debug)]
enum CurrentPane {
    PacketLeft,
    PacketRight,
    #[default]
    Search,
}

impl CurrentPane {
    fn next(&mut self) {
        *self = match self {
            CurrentPane::PacketLeft => CurrentPane::PacketRight,
            CurrentPane::PacketRight => CurrentPane::Search,
            CurrentPane::Search => CurrentPane::PacketLeft,
        }
    }

    fn previous(&mut self) {
        *self = match self {
            CurrentPane::PacketLeft => CurrentPane::Search,
            CurrentPane::PacketRight => CurrentPane::PacketLeft,
            CurrentPane::Search => CurrentPane::PacketRight,
        }
    }
}

#[derive(Debug, Default)]
pub struct App {
    counter: u8,
    exit: bool,
    pane: CurrentPane,
}

impl App {
    /// runs the application's main loop until the user quits
    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        while !self.exit {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events()?;
        }
        Ok(())
    }

    fn draw(&self, frame: &mut Frame) {
        frame.render_widget(self, frame.area());
    }

    /// updates the application's state based on user input
    fn handle_events(&mut self) -> io::Result<()> {
        #[allow(clippy::single_match)]
        match event::read()? {
            // it's important to check that the event is a key press event as
            // crossterm also emits key release and repeat events on Windows.
            Event::Key(key_event) => {
                if key_event.kind == KeyEventKind::Press {
                    self.handle_key_event(key_event)
                }
            }
            _ => {}
        };
        Ok(())
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Char('q') => self.exit(),
            KeyCode::Left => self.decrement_counter(),
            KeyCode::Right => self.increment_counter(),
            KeyCode::Tab => {
                self.pane.next();
            }
            KeyCode::BackTab => {
                self.pane.previous();
            }
            _ => {}
        }
    }

    fn exit(&mut self) {
        self.exit = true;
    }

    fn increment_counter(&mut self) {
        self.counter += 1;
    }

    fn decrement_counter(&mut self) {
        self.counter = self.counter.saturating_sub(1);
    }
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = Line::from(" Packet Slicer ".bold());

        let instructions = Line::from(vec![
            // " Decrement ".into(),
            // "<Left>".blue().bold(),
            // " Increment ".into(),
            // "<Right>".blue().bold(),
            " Quit ".into(),
            "<Q> ".blue().bold(),
        ]);
        let block = Block::bordered()
            .title(title.centered())
            .title_bottom(instructions.centered())
            .border_set(border::THICK);

        let counter_text = Text::from(vec![Line::from(vec![
            "Value: ".into(),
            self.counter.to_string().yellow(),
        ])]);

        Paragraph::new(counter_text)
            .centered()
            .block(block)
            .render(area, buf);
    }
}

fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let app_result = App::default().run(&mut terminal);
    ratatui::restore();
    app_result
}
