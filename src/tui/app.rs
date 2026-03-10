//! TUI application state and data refresh logic.

use std::path::PathBuf;

use crate::tui::widgets::toasts::Toast;
use ratatui::widgets::TableState;

use crate::db::events::EventStore;
use crate::db::mail::MailStore;
use crate::db::merge_queue::MergeQueue;
use crate::db::metrics::MetricsStore;
use crate::db::sessions::{RunStore, SessionStore};
use crate::types::{
    AgentSession, AgentState, MailFilters, MailMessage, MergeEntry, MergeEntryStatus, StoredEvent,
    TokenSnapshot,
};

// ---------------------------------------------------------------------------
// View / Focus enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    Overview,
    AgentDetail,
    EventLog,
    Terminal,
    SplitTerminal,
    MailReader,
    CostAnalytics,
    Timeline,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Focus {
    Agents,
    Feed,
    Mail,
}

impl Focus {
    pub fn next(&self) -> Focus {
        match self {
            Focus::Agents => Focus::Feed,
            Focus::Feed => Focus::Mail,
            Focus::Mail => Focus::Agents,
        }
    }

    pub fn prev(&self) -> Focus {
        match self {
            Focus::Agents => Focus::Mail,
            Focus::Feed => Focus::Agents,
            Focus::Mail => Focus::Feed,
        }
    }
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

pub struct App {
    pub project_root: String,
    pub running: bool,
    pub current_view: View,
    pub focus: Focus,

    // Data
    pub sessions: Vec<AgentSession>,
    pub events: Vec<StoredEvent>,
    pub messages: Vec<MailMessage>,
    pub merge_entries: Vec<MergeEntry>,
    pub metric_session_count: i64,
    pub total_cost: f64,
    pub snapshots: Vec<TokenSnapshot>,
    pub run_id: Option<String>,

    // UI state
    pub table_state: TableState,
    pub feed_scroll: usize,
    pub mail_scroll: usize,
    pub event_log_scroll: usize,
    pub detail_scroll: usize,

    // Filter
    pub filter_text: String,
    pub filter_mode: bool,

    // Overlays / subviews
    pub show_help: bool,
    pub show_completed: bool,
    pub selected_agent: Option<AgentSession>,
    pub agent_detail_events: Vec<StoredEvent>,
    pub agent_detail_mail: Vec<MailMessage>,

    // Mail reader
    pub selected_message: Option<MailMessage>,
    pub mail_reader_scroll: usize,
    pub thread_messages: Vec<MailMessage>,
    pub reply_mode: bool,
    pub reply_text: String,

    // Incremental event cursor
    pub last_event_id: i64,

    // Tick counter for staggered refresh
    pub tick_count: u64,

    // Terminal view state
    pub terminal_lines: Vec<String>,
    pub terminal_scroll: usize,
    pub terminal_fullscreen: bool,

    // Split terminal view state
    pub split_agents: Vec<AgentSession>,
    pub split_lines: Vec<Vec<String>>,
    pub split_focus: usize,

    // Toast notifications
    pub toasts: Vec<Toast>,
}

impl App {
    pub fn new(project_root: &str) -> Self {
        let mut table_state = TableState::default();
        table_state.select(Some(0));

        App {
            project_root: project_root.to_string(),
            running: true,
            current_view: View::Overview,
            focus: Focus::Agents,

            sessions: vec![],
            events: vec![],
            messages: vec![],
            merge_entries: vec![],
            metric_session_count: 0,
            total_cost: 0.0,
            snapshots: vec![],
            run_id: None,

            table_state,
            feed_scroll: 0,
            mail_scroll: 0,
            event_log_scroll: 0,
            detail_scroll: 0,

            filter_text: String::new(),
            filter_mode: false,

            show_help: false,
            show_completed: false,
            selected_agent: None,
            agent_detail_events: vec![],
            agent_detail_mail: vec![],

            selected_message: None,
            mail_reader_scroll: 0,
            thread_messages: vec![],
            reply_mode: false,
            reply_text: String::new(),

            last_event_id: 0,
            tick_count: 0,

            terminal_lines: vec![],
            terminal_scroll: 0,
            terminal_fullscreen: false,

            split_agents: vec![],
            split_lines: vec![],
            split_focus: 0,

            toasts: vec![],
        }
    }

    // -----------------------------------------------------------------------
    // DB paths
    // -----------------------------------------------------------------------

    fn sessions_db(&self) -> String {
        format!("{}/.overstory/sessions.db", self.project_root)
    }

    fn events_db(&self) -> String {
        format!("{}/.overstory/events.db", self.project_root)
    }

    fn mail_db(&self) -> String {
        format!("{}/.overstory/mail.db", self.project_root)
    }

    fn metrics_db(&self) -> String {
        format!("{}/.overstory/metrics.db", self.project_root)
    }

    fn merge_db(&self) -> String {
        format!("{}/.overstory/merge-queue.db", self.project_root)
    }

    fn run_id_file(&self) -> String {
        format!("{}/.overstory/current-run.txt", self.project_root)
    }

    // -----------------------------------------------------------------------
    // Data refresh
    // -----------------------------------------------------------------------

    pub fn refresh_all(&mut self) {
        self.refresh_run_id();
        self.refresh_sessions();
        self.refresh_events();
        self.refresh_mail();
        self.refresh_merge();
        self.refresh_metrics();
    }

    fn refresh_run_id(&mut self) {
        let path = self.run_id_file();
        if PathBuf::from(&path).exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                let trimmed = content.trim().to_string();
                if !trimmed.is_empty() {
                    self.run_id = Some(trimmed);
                    return;
                }
            }
        }
        // Try sessions.db for active run
        let sessions_path = self.sessions_db();
        if PathBuf::from(&sessions_path).exists() {
            if let Ok(store) = RunStore::new(&sessions_path) {
                if let Ok(Some(run)) = store.get_active_run() {
                    self.run_id = Some(run.id);
                }
            }
        }
    }

    fn refresh_sessions(&mut self) {
        let path = self.sessions_db();
        if !PathBuf::from(&path).exists() {
            return;
        }
        if let Ok(store) = SessionStore::new(&path) {
            if let Ok(mut sessions) = store.get_all() {
                sessions.sort_by_key(|s| state_priority(&s.state));
                self.sessions = sessions;
            }
        }
    }

    fn refresh_events(&mut self) {
        let path = self.events_db();
        if !PathBuf::from(&path).exists() {
            return;
        }
        if let Ok(store) = EventStore::new(&path) {
            // Fetch new events since last cursor
            if let Ok(new_events) = store.get_feed(None, None, Some(self.last_event_id), Some(200))
            {
                if let Some(last) = new_events.last() {
                    self.last_event_id = last.id;
                }
                // Append and keep last 500
                self.events.extend(new_events);
                if self.events.len() > 500 {
                    let drain = self.events.len() - 500;
                    self.events.drain(..drain);
                }
            }
        }
    }

    fn refresh_mail(&mut self) {
        let path = self.mail_db();
        if !PathBuf::from(&path).exists() {
            return;
        }
        if let Ok(store) = MailStore::new(&path) {
            if let Ok(msgs) = store.get_all(Some(MailFilters {
                limit: Some(50),
                ..Default::default()
            })) {
                self.messages = msgs;
            }
        }
    }

    fn refresh_merge(&mut self) {
        let path = self.merge_db();
        if !PathBuf::from(&path).exists() {
            return;
        }
        if let Ok(q) = MergeQueue::new(&path) {
            if let Ok(entries) = q.list(Some(MergeEntryStatus::Pending)) {
                self.merge_entries = entries;
            }
        }
    }

    fn refresh_metrics(&mut self) {
        let path = self.metrics_db();
        if !PathBuf::from(&path).exists() {
            return;
        }
        if let Ok(store) = MetricsStore::new(&path) {
            if let Ok(count) = store.count_sessions() {
                self.metric_session_count = count;
            }
            // Try current run first, fall back to all data if empty
            let mut snaps = store
                .get_latest_snapshots(self.run_id.as_deref())
                .unwrap_or_default();
            if snaps.is_empty() {
                snaps = store.get_latest_snapshots(None).unwrap_or_default();
            }
            self.total_cost = snaps.iter().filter_map(|s| s.estimated_cost_usd).sum();
            self.snapshots = snaps;
        }
    }

    fn refresh_agent_detail(&mut self, agent_name: &str) {
        // Events for this agent
        let events_path = self.events_db();
        if PathBuf::from(&events_path).exists() {
            if let Ok(store) = EventStore::new(&events_path) {
                if let Ok(evts) = store.get_by_agent(agent_name, None) {
                    // Take last 50
                    let start = evts.len().saturating_sub(50);
                    self.agent_detail_events = evts[start..].to_vec();
                }
            }
        }

        // Mail for this agent
        let mail_path = self.mail_db();
        if PathBuf::from(&mail_path).exists() {
            if let Ok(store) = MailStore::new(&mail_path) {
                let mut mail: Vec<MailMessage> = vec![];
                if let Ok(msgs) = store.get_all(Some(MailFilters {
                    from_agent: Some(agent_name.to_string()),
                    limit: Some(25),
                    ..Default::default()
                })) {
                    mail.extend(msgs);
                }
                if let Ok(msgs) = store.get_all(Some(MailFilters {
                    to_agent: Some(agent_name.to_string()),
                    limit: Some(25),
                    ..Default::default()
                })) {
                    mail.extend(msgs);
                }
                mail.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                mail.truncate(50);
                self.agent_detail_mail = mail;
            }
        }
    }

    // -----------------------------------------------------------------------
    // Tick
    // -----------------------------------------------------------------------

    pub fn tick(&mut self) {
        // Expire old toasts
        self.toasts.retain(|t| t.created.elapsed().as_secs() < 3);

        self.tick_count += 1;

        // Capture previous state for toast detection
        let prev_states: Vec<(String, AgentState)> = self
            .sessions
            .iter()
            .map(|s| (s.agent_name.clone(), s.state))
            .collect();
        let prev_mail_count = self.messages.len();

        // Sessions + events every tick (1s)
        self.refresh_sessions();
        self.refresh_events();

        // Toast detection: agent state changes
        for (name, old_state) in &prev_states {
            if let Some(session) = self.sessions.iter().find(|s| &s.agent_name == name) {
                if old_state != &session.state {
                    match session.state {
                        AgentState::Completed => self.toasts.push(Toast {
                            message: format!("✔ {} completed", name),
                            color: crate::tui::theme::ACCENT_GREEN,
                            created: std::time::Instant::now(),
                        }),
                        AgentState::Zombie => self.toasts.push(Toast {
                            message: format!("☠ {} died", name),
                            color: crate::tui::theme::ACCENT_RED,
                            created: std::time::Instant::now(),
                        }),
                        _ => {}
                    }
                }
            }
        }

        // Mail every 2s
        if self.tick_count.is_multiple_of(2) {
            self.refresh_mail();
            // Toast detection: new mail
            if self.messages.len() > prev_mail_count {
                if let Some(msg) = self.messages.first() {
                    self.toasts.push(Toast {
                        message: format!("✉ mail from {}", msg.from),
                        color: crate::tui::theme::ACCENT_PURPLE,
                        created: std::time::Instant::now(),
                    });
                }
            }
        }

        // Merge + metrics every 5s
        if self.tick_count.is_multiple_of(5) {
            self.refresh_merge();
            self.refresh_metrics();
            self.refresh_run_id();
        }

        // If in agent detail, refresh that data
        if self.current_view == View::AgentDetail {
            if let Some(ref agent) = self.selected_agent.clone() {
                self.refresh_agent_detail(&agent.agent_name.clone());
            }
        }

        // Terminal view: refresh tmux capture every tick
        if self.current_view == View::Terminal {
            if let Some(ref agent) = self.selected_agent.clone() {
                self.terminal_lines = capture_agent_output(
                    &agent.tmux_session,
                    &agent.agent_name,
                    &self.project_root,
                );
            }
        }

        // Split terminal view: refresh all split agents every tick
        if self.current_view == View::SplitTerminal {
            let agents = self.split_agents.clone();
            let project_root = self.project_root.clone();
            self.split_lines = agents
                .iter()
                .map(|a| capture_agent_output(&a.tmux_session, &a.agent_name, &project_root))
                .collect();
        }
    }

    // -----------------------------------------------------------------------
    // Input handling
    // -----------------------------------------------------------------------

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;

        // Filter mode intercepts most keys
        if self.filter_mode {
            match key.code {
                KeyCode::Esc => {
                    self.filter_mode = false;
                    self.filter_text.clear();
                }
                KeyCode::Enter => {
                    self.filter_mode = false;
                }
                KeyCode::Backspace => {
                    self.filter_text.pop();
                }
                KeyCode::Char(c) => {
                    self.filter_text.push(c);
                }
                _ => {}
            }
            return;
        }

        // Help overlay: ? toggles, Esc dismisses, other keys pass through
        if key.code == KeyCode::Char('?') {
            self.show_help = !self.show_help;
            return;
        }
        if self.show_help {
            if key.code == KeyCode::Esc || key.code == KeyCode::Char('q') {
                self.show_help = false;
            }
            return;
        }

        // Global view switch: 1 always goes to overview
        if key.code == KeyCode::Char('1') {
            self.current_view = View::Overview;
            self.selected_agent = None;
            return;
        }

        // View-specific handling
        match self.current_view {
            View::AgentDetail => self.handle_key_detail(key),
            View::EventLog => self.handle_key_event_log(key),
            View::Overview => self.handle_key_overview(key),
            View::Terminal => self.handle_key_terminal(key),
            View::SplitTerminal => self.handle_key_split_terminal(key),
            View::MailReader => self.handle_key_mail_reader(key),
            View::CostAnalytics => self.handle_key_cost_analytics(key),
            View::Timeline => self.handle_key_timeline(key),
        }
    }

    fn handle_key_overview(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::{KeyCode, KeyModifiers};

        match key.code {
            KeyCode::Char('q') => self.running = false,
            KeyCode::Char('r') => self.refresh_all(),
            KeyCode::Char('a') => self.show_completed = !self.show_completed,
            KeyCode::Char('/') => {
                self.filter_mode = true;
                self.filter_text.clear();
            }
            KeyCode::Tab => {
                self.focus = self.focus.next();
            }
            KeyCode::BackTab => {
                self.focus = self.focus.prev();
            }
            KeyCode::Char('1') => self.current_view = View::Overview,
            KeyCode::Char('2') => {
                self.current_view = View::EventLog;
                self.event_log_scroll = self.events.len().saturating_sub(1);
            }
            KeyCode::Char('4') | KeyCode::Char('$') => self.current_view = View::CostAnalytics,
            KeyCode::Char('5') => self.current_view = View::Timeline,
            KeyCode::Up | KeyCode::Char('k') => self.scroll_up(),
            KeyCode::Down | KeyCode::Char('j') => self.scroll_down(),
            KeyCode::Enter => {
                if self.focus == Focus::Mail {
                    self.enter_mail_reader();
                } else {
                    self.enter_detail();
                }
            }
            KeyCode::Char('t') => {
                if self.focus == Focus::Agents {
                    self.enter_terminal();
                }
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.running = false;
            }
            _ => {}
        }
    }

    fn handle_key_detail(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('b') => {
                self.current_view = View::Overview;
                self.selected_agent = None;
                self.detail_scroll = 0;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.detail_scroll = self.detail_scroll.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.detail_scroll += 1;
            }
            KeyCode::Char('t') => self.enter_terminal(),
            KeyCode::Char('q') => self.running = false,
            _ => {}
        }
    }

    fn handle_key_terminal(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.current_view = View::Overview;
                self.terminal_lines.clear();
                self.terminal_scroll = 0;
                self.terminal_fullscreen = false;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.terminal_scroll = self.terminal_scroll.saturating_add(1);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.terminal_scroll = self.terminal_scroll.saturating_sub(1);
            }
            KeyCode::Char('f') => {
                self.terminal_fullscreen = !self.terminal_fullscreen;
            }
            KeyCode::Char('s') => {
                self.enter_split();
            }
            KeyCode::Char('G') => {
                self.terminal_scroll = self.terminal_lines.len().saturating_sub(1);
            }
            KeyCode::Char('g') => {
                self.terminal_scroll = 0;
            }
            KeyCode::Char('q') => self.running = false,
            _ => {}
        }
    }

    fn handle_key_split_terminal(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.current_view = View::Overview;
            }
            KeyCode::Tab => {
                let count = self.split_agents.len();
                if count > 0 {
                    self.split_focus = (self.split_focus + 1) % count;
                }
            }
            KeyCode::Enter => {
                if let Some(agent) = self.split_agents.get(self.split_focus).cloned() {
                    self.selected_agent = Some(agent.clone());
                    self.terminal_lines = capture_agent_output(
                        &agent.tmux_session,
                        &agent.agent_name,
                        &self.project_root,
                    );
                    self.terminal_scroll = 0;
                    self.current_view = View::Terminal;
                }
            }
            KeyCode::Char('1') => {
                if !self.split_agents.is_empty() {
                    self.split_focus = 0;
                }
            }
            KeyCode::Char('2') => {
                if self.split_agents.len() >= 2 {
                    self.split_focus = 1;
                }
            }
            KeyCode::Char('3') => {
                if self.split_agents.len() >= 3 {
                    self.split_focus = 2;
                }
            }
            KeyCode::Char('4') => {
                if self.split_agents.len() >= 4 {
                    self.split_focus = 3;
                }
            }
            KeyCode::Char('q') => self.running = false,
            _ => {}
        }
    }

    fn handle_key_event_log(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('1') => {
                self.current_view = View::Overview;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.event_log_scroll = self.event_log_scroll.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.events.len().saturating_sub(1);
                if self.event_log_scroll < max {
                    self.event_log_scroll += 1;
                }
            }
            KeyCode::Char('G') => {
                self.event_log_scroll = self.events.len().saturating_sub(1);
            }
            KeyCode::Char('g') => {
                self.event_log_scroll = 0;
            }
            _ => {}
        }
    }

    fn handle_key_cost_analytics(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;
        match key.code {
            KeyCode::Esc => self.current_view = View::Overview,
            KeyCode::Char('q') => self.running = false,
            KeyCode::Up | KeyCode::Char('k') => { /* scroll if needed */ }
            KeyCode::Down | KeyCode::Char('j') => { /* scroll if needed */ }
            _ => {}
        }
    }

    fn handle_key_timeline(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;
        match key.code {
            KeyCode::Esc => self.current_view = View::Overview,
            KeyCode::Char('q') => self.running = false,
            KeyCode::Up | KeyCode::Char('k') => { /* scroll if needed */ }
            KeyCode::Down | KeyCode::Char('j') => { /* scroll if needed */ }
            _ => {}
        }
    }

    fn handle_key_mail_reader(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;

        // Reply compose mode intercepts keys
        if self.reply_mode {
            match key.code {
                KeyCode::Esc => {
                    self.reply_mode = false;
                    self.reply_text.clear();
                }
                KeyCode::Enter => {
                    self.send_reply();
                    self.reply_mode = false;
                    self.reply_text.clear();
                }
                KeyCode::Backspace => {
                    self.reply_text.pop();
                }
                KeyCode::Char(c) => {
                    self.reply_text.push(c);
                }
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('b') => {
                self.current_view = View::Overview;
                self.selected_message = None;
                self.thread_messages.clear();
                self.mail_reader_scroll = 0;
            }
            KeyCode::Char('r') => {
                self.reply_mode = true;
                self.reply_text.clear();
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.mail_reader_scroll = self.mail_reader_scroll.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.mail_reader_scroll += 1;
            }
            KeyCode::Char('g') => {
                self.mail_reader_scroll = 0;
            }
            KeyCode::Char('G') => {
                // Scroll to bottom — no easy max without render context, use large value
                self.mail_reader_scroll = usize::MAX / 2;
            }
            KeyCode::Char('q') => self.running = false,
            _ => {}
        }
    }

    fn enter_mail_reader(&mut self) {
        let idx = self.mail_scroll;
        if let Some(msg) = self.messages.get(idx).cloned() {
            // Mark as read
            let mail_path = self.mail_db();
            if std::path::PathBuf::from(&mail_path).exists() {
                if let Ok(store) = crate::db::mail::MailStore::new(&mail_path) {
                    let _ = store.mark_read(&msg.id);
                }
            }

            // Load thread if applicable
            let thread_id = msg.thread_id.clone().unwrap_or_else(|| msg.id.clone());
            let mail_path2 = self.mail_db();
            if std::path::PathBuf::from(&mail_path2).exists() {
                if let Ok(store) = crate::db::mail::MailStore::new(&mail_path2) {
                    if let Ok(thread) = store.get_by_thread(&thread_id) {
                        self.thread_messages = thread;
                    }
                }
            }

            self.selected_message = Some(msg);
            self.mail_reader_scroll = 0;
            self.reply_mode = false;
            self.reply_text.clear();
            self.current_view = View::MailReader;
        }
    }

    fn send_reply(&mut self) {
        use crate::types::{InsertMailMessage, MailMessageType, MailPriority};

        let body = self.reply_text.trim().to_string();
        if body.is_empty() {
            return;
        }

        let original = match &self.selected_message {
            Some(m) => m.clone(),
            None => return,
        };

        let thread_id = original
            .thread_id
            .clone()
            .unwrap_or_else(|| original.id.clone());

        let reply = InsertMailMessage {
            id: None,
            from_agent: original.to.clone(),
            to_agent: original.from.clone(),
            subject: format!("Re: {}", original.subject),
            body,
            priority: MailPriority::Normal,
            message_type: MailMessageType::Status,
            thread_id: Some(thread_id.clone()),
            payload: None,
        };

        let mail_path = self.mail_db();
        if std::path::PathBuf::from(&mail_path).exists() {
            if let Ok(store) = crate::db::mail::MailStore::new(&mail_path) {
                if let Ok(sent) = store.insert(&reply) {
                    // Refresh thread to include the new reply
                    if let Ok(thread) = store.get_by_thread(&thread_id) {
                        self.thread_messages = thread;
                    }
                    // Also refresh global messages list
                    self.refresh_mail();
                    // Update selected message's thread_id if it was just the message id
                    if let Some(ref mut msg) = self.selected_message {
                        if msg.thread_id.is_none() {
                            msg.thread_id = sent.thread_id;
                        }
                    }
                }
            }
        }
    }

    fn scroll_up(&mut self) {
        match self.focus {
            Focus::Agents => {
                let selected = self.table_state.selected().unwrap_or(0);
                if selected > 0 {
                    self.table_state.select(Some(selected - 1));
                }
            }
            Focus::Feed => {
                self.feed_scroll = self.feed_scroll.saturating_sub(1);
            }
            Focus::Mail => {
                self.mail_scroll = self.mail_scroll.saturating_sub(1);
            }
        }
    }

    fn scroll_down(&mut self) {
        match self.focus {
            Focus::Agents => {
                let visible = self.visible_sessions();
                let selected = self.table_state.selected().unwrap_or(0);
                if selected + 1 < visible.len() {
                    self.table_state.select(Some(selected + 1));
                }
            }
            Focus::Feed => {
                let max = self.events.len().saturating_sub(1);
                if self.feed_scroll < max {
                    self.feed_scroll += 1;
                }
            }
            Focus::Mail => {
                let max = self.messages.len().saturating_sub(1);
                if self.mail_scroll < max {
                    self.mail_scroll += 1;
                }
            }
        }
    }

    fn enter_detail(&mut self) {
        if self.focus != Focus::Agents {
            return;
        }
        let visible = self.visible_sessions();
        if let Some(idx) = self.table_state.selected() {
            if let Some(session) = visible.get(idx) {
                let agent_name = session.agent_name.clone();
                self.selected_agent = Some((*session).clone());
                self.detail_scroll = 0;
                self.refresh_agent_detail(&agent_name);
                self.current_view = View::AgentDetail;
            }
        }
    }

    // -----------------------------------------------------------------------
    // Terminal helpers
    // -----------------------------------------------------------------------

    pub fn enter_terminal(&mut self) {
        let agent = if let Some(ref a) = self.selected_agent {
            a.clone()
        } else {
            let visible = self.visible_sessions();
            match visible.get(self.table_state.selected().unwrap_or(0)) {
                Some(a) => (*a).clone(),
                None => return,
            }
        };
        self.selected_agent = Some(agent.clone());
        self.terminal_lines =
            capture_agent_output(&agent.tmux_session, &agent.agent_name, &self.project_root);
        self.terminal_scroll = 0;
        self.terminal_fullscreen = false;
        self.current_view = View::Terminal;
    }

    pub fn enter_split(&mut self) {
        let current = self.selected_agent.clone();
        let mut agents: Vec<AgentSession> = self
            .sessions
            .iter()
            .filter(|s| s.state == AgentState::Working || s.state == AgentState::Booting)
            .take(4)
            .cloned()
            .collect();

        // Ensure currently-viewed agent is included
        if let Some(ref cur) = current {
            if !agents.iter().any(|a| a.agent_name == cur.agent_name) {
                if agents.len() >= 4 {
                    agents.pop();
                }
                agents.insert(0, cur.clone());
            }
        }

        let project_root = self.project_root.clone();
        let lines: Vec<Vec<String>> = agents
            .iter()
            .map(|a| capture_agent_output(&a.tmux_session, &a.agent_name, &project_root))
            .collect();
        self.split_agents = agents;
        self.split_lines = lines;
        self.split_focus = 0;
        self.current_view = View::SplitTerminal;
    }

    // -----------------------------------------------------------------------
    // Derived data helpers
    // -----------------------------------------------------------------------

    /// Sessions filtered by name and optionally completed state.
    pub fn visible_sessions(&self) -> Vec<&AgentSession> {
        self.sessions
            .iter()
            .filter(|s| {
                if !self.show_completed && s.state == AgentState::Completed {
                    return false;
                }
                if !self.filter_text.is_empty() {
                    return s
                        .agent_name
                        .to_lowercase()
                        .contains(&self.filter_text.to_lowercase());
                }
                true
            })
            .collect()
    }

    /// Total cost across all snapshots.
    pub fn total_cost_display(&self) -> String {
        if self.total_cost > 0.0 {
            format!("${:.2}", self.total_cost)
        } else {
            String::new()
        }
    }

    /// Snapshot for a specific agent.
    pub fn snapshot_for(&self, agent_name: &str) -> Option<&TokenSnapshot> {
        self.snapshots.iter().find(|s| s.agent_name == agent_name)
    }

    /// Unread mail count.
    pub fn unread_count(&self) -> usize {
        self.messages.iter().filter(|m| !m.read).count()
    }

    /// Active agent count (working or booting).
    pub fn active_agent_count(&self) -> usize {
        self.sessions
            .iter()
            .filter(|s| s.state == AgentState::Working || s.state == AgentState::Booting)
            .count()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Capture agent output from log files.
///
/// Fallback chain:
/// 1. Read stdout.log from .overstory/logs/<agent>/<timestamp>/ (most recent subdir)
/// 2. If stdout.log is absent or empty, fall back to stderr.log (Codex writes to stderr)
/// 3. If neither exists, return a "no output available" message
pub fn capture_agent_output(
    _session_name: &str,
    agent_name: &str,
    project_root: &str,
) -> Vec<String> {
    let log_base = std::path::Path::new(project_root)
        .join(".overstory")
        .join("logs")
        .join(agent_name);

    if log_base.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&log_base) {
            let mut subdirs: Vec<std::path::PathBuf> = entries
                .flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| e.path())
                .collect();
            subdirs.sort();

            if let Some(latest) = subdirs.last() {
                // Try stdout.log first
                let stdout_log = latest.join("stdout.log");
                if let Ok(content) = std::fs::read_to_string(&stdout_log) {
                    if !content.trim().is_empty() {
                        let lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
                        let start = lines.len().saturating_sub(100);
                        return lines[start..].to_vec();
                    }
                }

                // Fall back to stderr.log (Codex and some other runtimes write here)
                let stderr_log = latest.join("stderr.log");
                if let Ok(content) = std::fs::read_to_string(&stderr_log) {
                    if !content.trim().is_empty() {
                        let lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
                        let start = lines.len().saturating_sub(100);
                        return lines[start..].to_vec();
                    }
                }
            }
        }
    }

    vec!["(no output available — agent log file not found)".to_string()]
}

pub fn state_priority(state: &AgentState) -> u8 {
    match state {
        AgentState::Working => 0,
        AgentState::Booting => 1,
        AgentState::Stalled => 2,
        AgentState::Zombie => 3,
        AgentState::Completed => 4,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AgentState;

    fn make_session(name: &str, state: AgentState) -> AgentSession {
        AgentSession {
            id: format!("session-{}", name),
            agent_name: name.to_string(),
            capability: "builder".to_string(),
            worktree_path: "/tmp".to_string(),
            branch_name: "main".to_string(),
            task_id: "task-1".to_string(),
            tmux_session: format!("tmux-{}", name),
            state,
            pid: None,
            parent_agent: None,
            depth: 1,
            run_id: None,
            started_at: "2024-01-01T00:00:00Z".to_string(),
            last_activity: "2024-01-01T00:00:01Z".to_string(),
            escalation_level: 0,
            stalled_since: None,
            transcript_path: None,
        }
    }

    #[test]
    fn test_app_new_default_state() {
        let app = App::new("/tmp");
        assert!(app.running);
        assert_eq!(app.current_view, View::Overview);
        assert_eq!(app.focus, Focus::Agents);
        assert!(app.sessions.is_empty());
        assert!(app.events.is_empty());
        assert!(!app.show_help);
        assert!(!app.show_completed);
        assert!(app.filter_text.is_empty());
    }

    #[test]
    fn test_state_priority_order() {
        assert!(state_priority(&AgentState::Working) < state_priority(&AgentState::Booting));
        assert!(state_priority(&AgentState::Booting) < state_priority(&AgentState::Stalled));
        assert!(state_priority(&AgentState::Stalled) < state_priority(&AgentState::Zombie));
        assert!(state_priority(&AgentState::Zombie) < state_priority(&AgentState::Completed));
    }

    #[test]
    fn test_focus_cycle() {
        assert_eq!(Focus::Agents.next(), Focus::Feed);
        assert_eq!(Focus::Feed.next(), Focus::Mail);
        assert_eq!(Focus::Mail.next(), Focus::Agents);
        assert_eq!(Focus::Agents.prev(), Focus::Mail);
        assert_eq!(Focus::Feed.prev(), Focus::Agents);
        assert_eq!(Focus::Mail.prev(), Focus::Feed);
    }

    #[test]
    fn test_visible_sessions_filter_completed() {
        let mut app = App::new("/tmp");
        app.sessions = vec![
            make_session("agent-a", AgentState::Working),
            make_session("agent-b", AgentState::Completed),
        ];

        // By default, completed agents are hidden
        let visible = app.visible_sessions();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].agent_name, "agent-a");

        // With show_completed, both visible
        app.show_completed = true;
        let visible = app.visible_sessions();
        assert_eq!(visible.len(), 2);
    }

    #[test]
    fn test_visible_sessions_filter_by_name() {
        let mut app = App::new("/tmp");
        app.sessions = vec![
            make_session("types-builder", AgentState::Working),
            make_session("config-scout", AgentState::Working),
        ];
        app.filter_text = "types".to_string();

        let visible = app.visible_sessions();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].agent_name, "types-builder");
    }

    #[test]
    fn test_active_agent_count() {
        let mut app = App::new("/tmp");
        app.sessions = vec![
            make_session("a", AgentState::Working),
            make_session("b", AgentState::Booting),
            make_session("c", AgentState::Completed),
            make_session("d", AgentState::Stalled),
        ];
        assert_eq!(app.active_agent_count(), 2);
    }

    #[test]
    fn test_unread_count() {
        use crate::types::{MailMessage, MailMessageType, MailPriority};
        let mut app = App::new("/tmp");
        app.messages = vec![
            MailMessage {
                id: "1".to_string(),
                from: "a".to_string(),
                to: "b".to_string(),
                subject: "test".to_string(),
                body: "body".to_string(),
                priority: MailPriority::Normal,
                message_type: MailMessageType::Status,
                thread_id: None,
                payload: None,
                read: false,
                created_at: "2024-01-01T00:00:00Z".to_string(),
            },
            MailMessage {
                id: "2".to_string(),
                from: "c".to_string(),
                to: "d".to_string(),
                subject: "read msg".to_string(),
                body: "body".to_string(),
                priority: MailPriority::Normal,
                message_type: MailMessageType::Status,
                thread_id: None,
                payload: None,
                read: true,
                created_at: "2024-01-01T00:00:01Z".to_string(),
            },
        ];
        assert_eq!(app.unread_count(), 1);
    }

    #[test]
    fn test_total_cost_display_empty() {
        let app = App::new("/tmp");
        assert_eq!(app.total_cost_display(), "");
    }

    #[test]
    fn test_total_cost_display_with_cost() {
        let mut app = App::new("/tmp");
        app.total_cost = 12.50;
        assert_eq!(app.total_cost_display(), "$12.50");
    }

    #[test]
    fn test_handle_key_quit() {
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
        let mut app = App::new("/tmp");
        assert!(app.running);
        let key = KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };
        app.handle_key(key);
        assert!(!app.running);
    }

    #[test]
    fn test_handle_key_help_toggle() {
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
        let mut app = App::new("/tmp");
        assert!(!app.show_help);
        let key = KeyEvent {
            code: KeyCode::Char('?'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };
        app.handle_key(key);
        assert!(app.show_help);
        // Any key dismisses help
        app.handle_key(key);
        assert!(!app.show_help);
    }

    #[test]
    fn test_handle_key_tab_cycles_focus() {
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
        let mut app = App::new("/tmp");
        assert_eq!(app.focus, Focus::Agents);
        let tab_key = KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };
        app.handle_key(tab_key);
        assert_eq!(app.focus, Focus::Feed);
        app.handle_key(tab_key);
        assert_eq!(app.focus, Focus::Mail);
        app.handle_key(tab_key);
        assert_eq!(app.focus, Focus::Agents);
    }

    #[test]
    fn test_empty_db_state_graceful() {
        // App with nonexistent project root should not panic
        let mut app = App::new("/nonexistent/path");
        app.refresh_all(); // Should silently handle missing DBs
        assert!(app.sessions.is_empty());
        assert!(app.events.is_empty());
        assert!(app.messages.is_empty());
    }

    #[test]
    fn test_handle_key_navigate_to_timeline() {
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
        let mut app = App::new("/tmp");
        assert_eq!(app.current_view, View::Overview);
        let key = KeyEvent {
            code: KeyCode::Char('5'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };
        app.handle_key(key);
        assert_eq!(app.current_view, View::Timeline);
    }

    #[test]
    fn test_capture_agent_output_no_session_no_log() {
        let lines = capture_agent_output("", "nonexistent-agent", "/nonexistent");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("no output available"));
    }

    #[test]
    fn test_capture_agent_output_reads_stdout_log() {
        use std::fs;
        let dir = tempfile::TempDir::new().unwrap();
        let log_dir = dir
            .path()
            .join(".overstory/logs/test-agent/2024-01-01T00:00:00");
        fs::create_dir_all(&log_dir).unwrap();
        fs::write(log_dir.join("stdout.log"), "line1\nline2\nline3\n").unwrap();

        let lines = capture_agent_output("", "test-agent", dir.path().to_str().unwrap());
        assert_eq!(lines, vec!["line1", "line2", "line3"]);
    }

    #[test]
    fn test_capture_agent_output_falls_back_to_stderr() {
        use std::fs;
        let dir = tempfile::TempDir::new().unwrap();
        let log_dir = dir
            .path()
            .join(".overstory/logs/codex-agent/2024-01-01T00:00:00");
        fs::create_dir_all(&log_dir).unwrap();
        // stdout.log is empty (as Codex produces), stderr.log has content
        fs::write(log_dir.join("stdout.log"), "").unwrap();
        fs::write(log_dir.join("stderr.log"), "codex output\nmore output\n").unwrap();

        let lines = capture_agent_output("", "codex-agent", dir.path().to_str().unwrap());
        assert_eq!(lines, vec!["codex output", "more output"]);
    }

    #[test]
    fn test_capture_agent_output_prefers_stdout_when_both_exist() {
        use std::fs;
        let dir = tempfile::TempDir::new().unwrap();
        let log_dir = dir
            .path()
            .join(".overstory/logs/mixed-agent/2024-01-01T00:00:00");
        fs::create_dir_all(&log_dir).unwrap();
        fs::write(log_dir.join("stdout.log"), "stdout content\n").unwrap();
        fs::write(log_dir.join("stderr.log"), "stderr content\n").unwrap();

        let lines = capture_agent_output("", "mixed-agent", dir.path().to_str().unwrap());
        assert_eq!(lines, vec!["stdout content"]);
    }
}
