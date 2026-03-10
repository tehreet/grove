//! TUI application state and data refresh logic.

use std::path::PathBuf;

use ratatui::widgets::TableState;

use crate::db::events::EventStore;
use crate::db::mail::MailStore;
use crate::db::merge_queue::MergeQueue;
use crate::db::metrics::MetricsStore;
use crate::db::sessions::{RunStore, SessionStore};
use crate::types::{
    AgentSession, AgentState, MailFilters, MailMessage, MergeEntry, MergeEntryStatus,
    StoredEvent, TokenSnapshot,
};

// ---------------------------------------------------------------------------
// View / Focus enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    Overview,
    AgentDetail,
    EventLog,
}

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

    // Incremental event cursor
    pub last_event_id: i64,

    // Tick counter for staggered refresh
    pub tick_count: u64,
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

            last_event_id: 0,
            tick_count: 0,
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
            if let Ok(new_events) = store.get_feed(None, None, Some(self.last_event_id), Some(200)) {
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
            if let Ok(snaps) = store.get_latest_snapshots(self.run_id.as_deref()) {
                self.total_cost = snaps
                    .iter()
                    .filter_map(|s| s.estimated_cost_usd)
                    .sum();
                self.snapshots = snaps;
            }
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
        self.tick_count += 1;

        // Sessions + events every tick (1s)
        self.refresh_sessions();
        self.refresh_events();

        // Mail every 2s
        if self.tick_count.is_multiple_of(2) {
            self.refresh_mail();
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

        // Help overlay: any key dismisses
        if self.show_help {
            self.show_help = false;
            return;
        }

        // View-specific handling
        match self.current_view {
            View::AgentDetail => self.handle_key_detail(key),
            View::EventLog => self.handle_key_event_log(key),
            View::Overview => self.handle_key_overview(key),
        }
    }

    fn handle_key_overview(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::{KeyCode, KeyModifiers};

        match key.code {
            KeyCode::Char('q') => self.running = false,
            KeyCode::Char('?') => self.show_help = true,
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
            KeyCode::Char('3') => self.show_help = true,
            KeyCode::Up | KeyCode::Char('k') => self.scroll_up(),
            KeyCode::Down | KeyCode::Char('j') => self.scroll_down(),
            KeyCode::Enter => self.enter_detail(),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.running = false;
            }
            _ => {}
        }
    }

    fn handle_key_detail(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc | KeyCode::Backspace => {
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
                    return s.agent_name.to_lowercase().contains(&self.filter_text.to_lowercase());
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

pub fn state_priority(state: &AgentState) -> u8 {
    match state {
        AgentState::Working => 0,
        AgentState::Booting => 1,
        AgentState::Stalled => 2,
        AgentState::Zombie => 3,
        AgentState::Completed => 4,
    }
}
