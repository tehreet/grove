//! TUI widget modules.

pub mod agent_card;
// agent_table is kept for future use / other views; overview now uses agent_card grid.
#[allow(dead_code)]
pub mod agent_table;
pub mod feed;
pub mod header;
pub mod mail_list;
pub mod status_bar;
// toasts is not yet wired into app.rs; Builder 1 integrates after merge.
#[allow(dead_code)]
pub mod toasts;
