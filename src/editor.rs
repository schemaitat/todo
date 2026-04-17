use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::cell::Cell;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditorMode {
    Normal,
    Insert,
    Visual,
    Command,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditorExit {
    Save,
    Cancel,
}

#[derive(Clone)]
struct Snap {
    lines: Vec<String>,
    row: usize,
    col: usize,
}

pub struct VimEditor {
    pub lines: Vec<String>,
    pub row: usize,
    pub col: usize,
    pub mode: EditorMode,
    pub command_buffer: String,
    pub status: String,
    pub exit: Option<EditorExit>,
    pub visual_anchor: Option<(usize, usize)>,
    pub pending: Option<char>,
    pub scroll: Cell<usize>,
    pub viewport_height: Cell<usize>,
    register: String,
    register_linewise: bool,
    undo: Vec<Snap>,
    redo: Vec<Snap>,
}

impl VimEditor {
    pub fn new(body: &str) -> Self {
        let mut lines: Vec<String> = body.split('\n').map(|s| s.to_string()).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        Self {
            lines,
            row: 0,
            col: 0,
            mode: EditorMode::Normal,
            command_buffer: String::new(),
            status: String::new(),
            exit: None,
            visual_anchor: None,
            pending: None,
            scroll: Cell::new(0),
            viewport_height: Cell::new(20),
            register: String::new(),
            register_linewise: false,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    pub fn body(&self) -> String {
        self.lines.join("\n")
    }

    pub fn visual_range(&self) -> Option<((usize, usize), (usize, usize))> {
        let (ar, ac) = self.visual_anchor?;
        let a = (ar, ac);
        let c = (self.row, self.col);
        Some(if a <= c { (a, c) } else { (c, a) })
    }

    fn snap(&self) -> Snap {
        Snap {
            lines: self.lines.clone(),
            row: self.row,
            col: self.col,
        }
    }

    fn push_undo(&mut self) {
        self.undo.push(self.snap());
        self.redo.clear();
        if self.undo.len() > 500 {
            self.undo.remove(0);
        }
    }

    fn do_undo(&mut self) {
        if let Some(s) = self.undo.pop() {
            self.redo.push(self.snap());
            self.lines = s.lines;
            self.row = s.row;
            self.col = s.col;
            self.clamp_col();
        }
    }

    fn do_redo(&mut self) {
        if let Some(s) = self.redo.pop() {
            self.undo.push(self.snap());
            self.lines = s.lines;
            self.row = s.row;
            self.col = s.col;
            self.clamp_col();
        }
    }

    fn line_char_len(&self, r: usize) -> usize {
        self.lines.get(r).map(|l| l.chars().count()).unwrap_or(0)
    }

    fn clamp_col(&mut self) {
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        if self.row >= self.lines.len() {
            self.row = self.lines.len() - 1;
        }
        let len = self.line_char_len(self.row);
        let max = match self.mode {
            EditorMode::Insert => len,
            _ => len.saturating_sub(if len == 0 { 0 } else { 1 }),
        };
        if self.col > max {
            self.col = max;
        }
    }

    fn byte_col(line: &str, c: usize) -> usize {
        line.char_indices()
            .nth(c)
            .map(|(b, _)| b)
            .unwrap_or(line.len())
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match self.mode {
            EditorMode::Normal => self.handle_normal(key),
            EditorMode::Insert => self.handle_insert(key),
            EditorMode::Visual => self.handle_visual(key),
            EditorMode::Command => self.handle_command_mode(key),
        }
        self.clamp_col();
    }

    fn handle_normal(&mut self, key: KeyEvent) {
        let pending = self.pending.take();
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        if let Some(p) = pending {
            if self.handle_pending(p, key) {
                return;
            }
        }

        match (key.code, ctrl) {
            (KeyCode::Esc, _) => {
                self.pending = None;
                self.status.clear();
            }
            (KeyCode::Char('h'), false) | (KeyCode::Left, _) => self.move_left(),
            (KeyCode::Char('l'), false) | (KeyCode::Right, _) => self.move_right(),
            (KeyCode::Char('j'), false) | (KeyCode::Down, _) => self.move_down(),
            (KeyCode::Char('k'), false) | (KeyCode::Up, _) => self.move_up(),
            (KeyCode::Char('0'), false) | (KeyCode::Home, _) => self.col = 0,
            (KeyCode::Char('$'), false) | (KeyCode::End, _) => {
                self.col = self.line_char_len(self.row).saturating_sub(1);
            }
            (KeyCode::Char('^'), false) => self.col = self.first_nonblank(self.row),
            (KeyCode::Char('w'), false) => self.move_word_forward(true),
            (KeyCode::Char('W'), false) => self.move_word_forward(false),
            (KeyCode::Char('b'), false) => self.move_word_back(true),
            (KeyCode::Char('B'), false) => self.move_word_back(false),
            (KeyCode::Char('e'), false) => self.move_word_end(true),
            (KeyCode::Char('E'), false) => self.move_word_end(false),
            (KeyCode::Char('G'), false) => {
                self.row = self.lines.len().saturating_sub(1);
                self.col = 0;
            }
            (KeyCode::Char('g'), false) => self.pending = Some('g'),
            (KeyCode::Char('d'), false) => self.pending = Some('d'),
            (KeyCode::Char('y'), false) => self.pending = Some('y'),
            (KeyCode::Char('c'), false) => self.pending = Some('c'),
            (KeyCode::Char('r'), false) => self.pending = Some('r'),
            (KeyCode::Char('u'), false) => self.do_undo(),
            (KeyCode::Char('r'), true) => self.do_redo(),
            (KeyCode::Char('d'), true) => self.scroll_half_down(),
            (KeyCode::Char('u'), true) => self.scroll_half_up(),
            (KeyCode::Char('i'), false) => self.enter_insert(),
            (KeyCode::Char('I'), false) => {
                self.col = self.first_nonblank(self.row);
                self.enter_insert();
            }
            (KeyCode::Char('a'), false) => {
                if self.line_char_len(self.row) > 0 {
                    self.col += 1;
                }
                self.enter_insert();
            }
            (KeyCode::Char('A'), false) => {
                self.col = self.line_char_len(self.row);
                self.enter_insert();
            }
            (KeyCode::Char('o'), false) => {
                self.push_undo();
                self.row += 1;
                self.lines.insert(self.row, String::new());
                self.col = 0;
                self.enter_insert();
            }
            (KeyCode::Char('O'), false) => {
                self.push_undo();
                self.lines.insert(self.row, String::new());
                self.col = 0;
                self.enter_insert();
            }
            (KeyCode::Char('x'), false) => self.delete_char_under(),
            (KeyCode::Char('X'), false) => {
                if self.col > 0 {
                    self.col -= 1;
                    self.delete_char_under();
                }
            }
            (KeyCode::Char('D'), false) => self.delete_to_end_of_line(),
            (KeyCode::Char('C'), false) => {
                self.delete_to_end_of_line();
                self.enter_insert();
            }
            (KeyCode::Char('p'), false) => self.paste(true),
            (KeyCode::Char('P'), false) => self.paste(false),
            (KeyCode::Char('v'), false) => {
                self.mode = EditorMode::Visual;
                self.visual_anchor = Some((self.row, self.col));
                self.status = String::from("-- VISUAL --");
            }
            (KeyCode::Char(':'), false) => {
                self.mode = EditorMode::Command;
                self.command_buffer.clear();
            }
            _ => {}
        }
    }

    fn handle_pending(&mut self, p: char, key: KeyEvent) -> bool {
        match (p, key.code) {
            ('g', KeyCode::Char('g')) => {
                self.row = 0;
                self.col = 0;
                true
            }
            ('d', KeyCode::Char('d')) => {
                self.delete_line();
                true
            }
            ('y', KeyCode::Char('y')) => {
                self.yank_line();
                true
            }
            ('c', KeyCode::Char('c')) => {
                self.push_undo();
                if let Some(l) = self.lines.get_mut(self.row) {
                    l.clear();
                }
                self.col = 0;
                self.enter_insert();
                true
            }
            ('d', KeyCode::Char('w')) => {
                self.push_undo();
                let (r, c) = self.word_forward_target(self.row, self.col, true);
                self.delete_range((self.row, self.col), (r, c));
                true
            }
            ('c', KeyCode::Char('w')) => {
                self.push_undo();
                let (r, c) = self.word_forward_target(self.row, self.col, true);
                self.delete_range((self.row, self.col), (r, c));
                self.enter_insert();
                true
            }
            ('y', KeyCode::Char('w')) => {
                let (r, c) = self.word_forward_target(self.row, self.col, true);
                self.yank_range((self.row, self.col), (r, c));
                true
            }
            ('d', KeyCode::Char('$')) => {
                self.delete_to_end_of_line();
                true
            }
            ('c', KeyCode::Char('$')) => {
                self.delete_to_end_of_line();
                self.enter_insert();
                true
            }
            ('d', KeyCode::Char('0')) => {
                self.push_undo();
                self.delete_range((self.row, 0), (self.row, self.col));
                self.col = 0;
                true
            }
            ('r', KeyCode::Char(c)) => {
                self.push_undo();
                self.replace_char(c);
                true
            }
            _ => {
                self.pending = None;
                false
            }
        }
    }

    fn handle_insert(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = EditorMode::Normal;
                if self.col > 0 && self.col >= self.line_char_len(self.row) {
                    self.col = self.line_char_len(self.row).saturating_sub(1);
                }
                self.status.clear();
            }
            KeyCode::Enter => {
                self.push_undo();
                let line = self.lines[self.row].clone();
                let b = Self::byte_col(&line, self.col);
                let (left, right) = line.split_at(b);
                self.lines[self.row] = left.to_string();
                self.lines.insert(self.row + 1, right.to_string());
                self.row += 1;
                self.col = 0;
            }
            KeyCode::Backspace => {
                if self.col > 0 {
                    self.push_undo();
                    let line = &mut self.lines[self.row];
                    let b = Self::byte_col(line, self.col - 1);
                    let bn = Self::byte_col(line, self.col);
                    line.replace_range(b..bn, "");
                    self.col -= 1;
                } else if self.row > 0 {
                    self.push_undo();
                    let cur = self.lines.remove(self.row);
                    self.row -= 1;
                    self.col = self.line_char_len(self.row);
                    self.lines[self.row].push_str(&cur);
                }
            }
            KeyCode::Tab => {
                self.push_undo();
                let spaces = "    ";
                let line = &mut self.lines[self.row];
                let b = Self::byte_col(line, self.col);
                line.insert_str(b, spaces);
                self.col += 4;
            }
            KeyCode::Delete => self.delete_char_under(),
            KeyCode::Left => self.move_left(),
            KeyCode::Right => {
                let len = self.line_char_len(self.row);
                if self.col < len {
                    self.col += 1;
                }
            }
            KeyCode::Up => self.move_up(),
            KeyCode::Down => self.move_down(),
            KeyCode::Home => self.col = 0,
            KeyCode::End => self.col = self.line_char_len(self.row),
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.push_undo();
                let line = &mut self.lines[self.row];
                let b = Self::byte_col(line, self.col);
                line.insert(b, c);
                self.col += 1;
            }
            _ => {}
        }
    }

    fn handle_visual(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match (key.code, ctrl) {
            (KeyCode::Esc, _) | (KeyCode::Char('v'), false) => {
                self.mode = EditorMode::Normal;
                self.visual_anchor = None;
                self.status.clear();
            }
            (KeyCode::Char('h'), false) | (KeyCode::Left, _) => self.move_left(),
            (KeyCode::Char('l'), false) | (KeyCode::Right, _) => self.move_right_visual(),
            (KeyCode::Char('j'), false) | (KeyCode::Down, _) => self.move_down(),
            (KeyCode::Char('k'), false) | (KeyCode::Up, _) => self.move_up(),
            (KeyCode::Char('0'), false) => self.col = 0,
            (KeyCode::Char('$'), false) => {
                self.col = self.line_char_len(self.row).saturating_sub(1);
            }
            (KeyCode::Char('^'), false) => self.col = self.first_nonblank(self.row),
            (KeyCode::Char('w'), false) => self.move_word_forward(true),
            (KeyCode::Char('b'), false) => self.move_word_back(true),
            (KeyCode::Char('e'), false) => self.move_word_end(true),
            (KeyCode::Char('g'), false) => self.pending = Some('g'),
            (KeyCode::Char('G'), false) => {
                self.row = self.lines.len().saturating_sub(1);
                self.col = 0;
            }
            (KeyCode::Char('y'), false) => {
                if let Some((s, e)) = self.visual_range() {
                    self.yank_range(s, self.next_pos(e));
                }
                self.mode = EditorMode::Normal;
                self.visual_anchor = None;
            }
            (KeyCode::Char('d'), false) | (KeyCode::Char('x'), false) => {
                if let Some((s, e)) = self.visual_range() {
                    self.push_undo();
                    self.delete_range(s, self.next_pos(e));
                }
                self.mode = EditorMode::Normal;
                self.visual_anchor = None;
            }
            (KeyCode::Char('c'), false) => {
                if let Some((s, e)) = self.visual_range() {
                    self.push_undo();
                    self.delete_range(s, self.next_pos(e));
                }
                self.visual_anchor = None;
                self.enter_insert();
            }
            _ => {
                if let Some(p) = self.pending.take() {
                    if p == 'g' && matches!(key.code, KeyCode::Char('g')) {
                        self.row = 0;
                        self.col = 0;
                    }
                }
            }
        }
    }

    fn handle_command_mode(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = EditorMode::Normal;
                self.command_buffer.clear();
            }
            KeyCode::Enter => {
                let cmd = std::mem::take(&mut self.command_buffer);
                self.mode = EditorMode::Normal;
                self.run_ex(cmd.trim());
            }
            KeyCode::Backspace => {
                if self.command_buffer.pop().is_none() {
                    self.mode = EditorMode::Normal;
                }
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.command_buffer.push(c);
            }
            _ => {}
        }
    }

    fn run_ex(&mut self, cmd: &str) {
        match cmd {
            "w" | "write" => {
                self.exit = Some(EditorExit::Save);
            }
            "q" | "quit" => {
                self.exit = Some(EditorExit::Cancel);
            }
            "q!" => {
                self.exit = Some(EditorExit::Cancel);
            }
            "wq" | "x" | "wq!" => {
                self.exit = Some(EditorExit::Save);
            }
            "" => {}
            other => {
                self.status = format!("unknown command: {}", other);
            }
        }
    }

    fn enter_insert(&mut self) {
        self.mode = EditorMode::Insert;
        self.status = String::from("-- INSERT --");
    }

    fn move_left(&mut self) {
        self.col = self.col.saturating_sub(1);
    }
    fn move_right(&mut self) {
        let len = self.line_char_len(self.row);
        let max = len.saturating_sub(if len == 0 { 0 } else { 1 });
        if self.col < max {
            self.col += 1;
        }
    }
    fn move_right_visual(&mut self) {
        let len = self.line_char_len(self.row);
        let max = len.saturating_sub(if len == 0 { 0 } else { 1 });
        if self.col < max {
            self.col += 1;
        }
    }
    fn move_up(&mut self) {
        if self.row > 0 {
            self.row -= 1;
        }
    }
    fn move_down(&mut self) {
        if self.row + 1 < self.lines.len() {
            self.row += 1;
        }
    }

    fn first_nonblank(&self, r: usize) -> usize {
        self.lines
            .get(r)
            .map(|l| l.chars().take_while(|c| c.is_whitespace()).count())
            .unwrap_or(0)
    }

    fn char_class_word(c: char) -> u8 {
        if c.is_whitespace() {
            0
        } else if c.is_alphanumeric() || c == '_' {
            1
        } else {
            2
        }
    }
    fn char_class_ws(c: char) -> u8 {
        if c.is_whitespace() {
            0
        } else {
            1
        }
    }

    fn char_at(&self, r: usize, c: usize) -> Option<char> {
        self.lines.get(r)?.chars().nth(c)
    }

    fn next_pos(&self, p: (usize, usize)) -> (usize, usize) {
        let (r, c) = p;
        let len = self.line_char_len(r);
        if c < len {
            (r, c + 1)
        } else if r + 1 < self.lines.len() {
            (r + 1, 0)
        } else {
            (r, c)
        }
    }

    fn word_forward_target(&self, mut r: usize, mut c: usize, word: bool) -> (usize, usize) {
        let klass = |ch: char| if word { Self::char_class_word(ch) } else { Self::char_class_ws(ch) };
        let start_class = self.char_at(r, c).map(klass).unwrap_or(0);
        loop {
            let len = self.line_char_len(r);
            if c + 1 >= len {
                if r + 1 >= self.lines.len() {
                    c = len;
                    break;
                }
                r += 1;
                c = 0;
            } else {
                c += 1;
            }
            let Some(ch) = self.char_at(r, c) else { break };
            let k = klass(ch);
            if k != 0 && k != start_class {
                break;
            }
            if start_class != 0 && k == 0 {
                while let Some(ch2) = self.char_at(r, c) {
                    if !ch2.is_whitespace() {
                        break;
                    }
                    let len = self.line_char_len(r);
                    if c + 1 >= len {
                        if r + 1 >= self.lines.len() {
                            return (r, c);
                        }
                        r += 1;
                        c = 0;
                    } else {
                        c += 1;
                    }
                }
                break;
            }
        }
        (r, c)
    }

    fn move_word_forward(&mut self, word: bool) {
        let (r, c) = self.word_forward_target(self.row, self.col, word);
        self.row = r;
        self.col = c;
    }

    fn move_word_back(&mut self, word: bool) {
        let klass = |ch: char| if word { Self::char_class_word(ch) } else { Self::char_class_ws(ch) };
        let (mut r, mut c) = (self.row, self.col);
        loop {
            if c == 0 {
                if r == 0 {
                    break;
                }
                r -= 1;
                c = self.line_char_len(r);
                if c > 0 {
                    c -= 1;
                } else {
                    continue;
                }
            } else {
                c -= 1;
            }
            let Some(ch) = self.char_at(r, c) else { continue };
            if klass(ch) != 0 {
                // walk back through same class
                while c > 0 {
                    let Some(prev) = self.char_at(r, c - 1) else { break };
                    if klass(prev) == klass(ch) {
                        c -= 1;
                    } else {
                        break;
                    }
                }
                break;
            }
        }
        self.row = r;
        self.col = c;
    }

    fn move_word_end(&mut self, word: bool) {
        let klass = |ch: char| if word { Self::char_class_word(ch) } else { Self::char_class_ws(ch) };
        let (mut r, mut c) = (self.row, self.col);
        let len = self.line_char_len(r);
        if c + 1 < len {
            c += 1;
        } else if r + 1 < self.lines.len() {
            r += 1;
            c = 0;
        }
        // skip whitespace
        while let Some(ch) = self.char_at(r, c) {
            if !ch.is_whitespace() {
                break;
            }
            let len = self.line_char_len(r);
            if c + 1 >= len {
                if r + 1 >= self.lines.len() {
                    self.row = r;
                    self.col = c;
                    return;
                }
                r += 1;
                c = 0;
            } else {
                c += 1;
            }
        }
        let Some(start_ch) = self.char_at(r, c) else {
            self.row = r;
            self.col = c;
            return;
        };
        let k = klass(start_ch);
        loop {
            let len = self.line_char_len(r);
            if c + 1 >= len {
                break;
            }
            let Some(next) = self.char_at(r, c + 1) else { break };
            if klass(next) != k {
                break;
            }
            c += 1;
        }
        self.row = r;
        self.col = c;
    }

    fn delete_char_under(&mut self) {
        let len = self.line_char_len(self.row);
        if len == 0 {
            return;
        }
        self.push_undo();
        let line = &mut self.lines[self.row];
        let b = Self::byte_col(line, self.col);
        let bn = Self::byte_col(line, self.col + 1);
        let removed: String = line[b..bn].to_string();
        line.replace_range(b..bn, "");
        self.register = removed;
        self.register_linewise = false;
        let nlen = self.line_char_len(self.row);
        if self.col >= nlen && nlen > 0 {
            self.col = nlen - 1;
        }
    }

    fn delete_to_end_of_line(&mut self) {
        self.push_undo();
        let line = &mut self.lines[self.row];
        let b = Self::byte_col(line, self.col);
        self.register = line[b..].to_string();
        self.register_linewise = false;
        line.truncate(b);
        let len = self.line_char_len(self.row);
        if self.col > 0 && self.col >= len {
            self.col = len.saturating_sub(1);
        }
    }

    fn delete_line(&mut self) {
        self.push_undo();
        let removed = self.lines.remove(self.row);
        self.register = format!("{}\n", removed);
        self.register_linewise = true;
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        if self.row >= self.lines.len() {
            self.row = self.lines.len() - 1;
        }
        self.col = self.first_nonblank(self.row);
    }

    fn yank_line(&mut self) {
        let line = self.lines.get(self.row).cloned().unwrap_or_default();
        self.register = format!("{}\n", line);
        self.register_linewise = true;
    }

    fn replace_char(&mut self, c: char) {
        let len = self.line_char_len(self.row);
        if len == 0 {
            return;
        }
        let line = &mut self.lines[self.row];
        let b = Self::byte_col(line, self.col);
        let bn = Self::byte_col(line, self.col + 1);
        line.replace_range(b..bn, &c.to_string());
    }

    fn delete_range(&mut self, start: (usize, usize), end: (usize, usize)) {
        let (sr, sc) = start;
        let (er, ec) = end;
        if (sr, sc) == (er, ec) {
            return;
        }
        if sr == er {
            let line = &mut self.lines[sr];
            let bs = Self::byte_col(line, sc);
            let be = Self::byte_col(line, ec);
            self.register = line[bs..be].to_string();
            line.replace_range(bs..be, "");
        } else {
            let first = self.lines[sr].clone();
            let last = self.lines[er].clone();
            let bs = Self::byte_col(&first, sc);
            let be = Self::byte_col(&last, ec);
            let mut removed = first[bs..].to_string();
            removed.push('\n');
            for i in (sr + 1)..er {
                removed.push_str(&self.lines[i]);
                removed.push('\n');
            }
            removed.push_str(&last[..be]);
            self.register = removed;
            let new_first = format!("{}{}", &first[..bs], &last[be..]);
            self.lines[sr] = new_first;
            self.lines.drain((sr + 1)..=er);
        }
        self.register_linewise = false;
        self.row = sr;
        self.col = sc;
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
    }

    fn yank_range(&mut self, start: (usize, usize), end: (usize, usize)) {
        let (sr, sc) = start;
        let (er, ec) = end;
        if (sr, sc) == (er, ec) {
            return;
        }
        if sr == er {
            let line = &self.lines[sr];
            let bs = Self::byte_col(line, sc);
            let be = Self::byte_col(line, ec);
            self.register = line[bs..be].to_string();
        } else {
            let first = &self.lines[sr];
            let last = &self.lines[er];
            let bs = Self::byte_col(first, sc);
            let be = Self::byte_col(last, ec);
            let mut out = first[bs..].to_string();
            out.push('\n');
            for i in (sr + 1)..er {
                out.push_str(&self.lines[i]);
                out.push('\n');
            }
            out.push_str(&last[..be]);
            self.register = out;
        }
        self.register_linewise = false;
    }

    fn paste(&mut self, after: bool) {
        if self.register.is_empty() {
            return;
        }
        self.push_undo();
        if self.register_linewise {
            let text = self.register.trim_end_matches('\n').to_string();
            let new_lines: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
            let insert_at = if after { self.row + 1 } else { self.row };
            for (i, l) in new_lines.iter().enumerate() {
                self.lines.insert(insert_at + i, l.clone());
            }
            self.row = insert_at;
            self.col = self.first_nonblank(self.row);
        } else {
            let insert_col = if after && self.line_char_len(self.row) > 0 {
                self.col + 1
            } else {
                self.col
            };
            let reg = self.register.clone();
            let pieces: Vec<&str> = reg.split('\n').collect();
            if pieces.len() == 1 {
                let line = &mut self.lines[self.row];
                let b = Self::byte_col(line, insert_col);
                line.insert_str(b, pieces[0]);
                self.col = insert_col + pieces[0].chars().count().saturating_sub(1);
            } else {
                let line = self.lines[self.row].clone();
                let b = Self::byte_col(&line, insert_col);
                let left = line[..b].to_string();
                let right = line[b..].to_string();
                let mut new = left;
                new.push_str(pieces[0]);
                self.lines[self.row] = new;
                for (i, piece) in pieces[1..pieces.len() - 1].iter().enumerate() {
                    self.lines.insert(self.row + 1 + i, piece.to_string());
                }
                let last_idx = self.row + pieces.len() - 1;
                let mut last = pieces[pieces.len() - 1].to_string();
                last.push_str(&right);
                self.lines.insert(last_idx, last);
                self.row = last_idx;
                self.col = pieces[pieces.len() - 1].chars().count();
            }
        }
    }

    fn scroll_half_down(&mut self) {
        let h = self.viewport_height.get().max(1);
        let half = (h / 2).max(1);
        self.row = (self.row + half).min(self.lines.len().saturating_sub(1));
    }
    fn scroll_half_up(&mut self) {
        let h = self.viewport_height.get().max(1);
        let half = (h / 2).max(1);
        self.row = self.row.saturating_sub(half);
    }
}
