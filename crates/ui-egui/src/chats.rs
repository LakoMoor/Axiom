//! Earlier chats, listed in the sidebar like the ChatGPT app's history. A chat is kept as its
//! questions and answers only (proof cards and pending approvals belong to the moment they were
//! made); it is saved with the other preferences, so it is there after a restart.

use crate::CraftApp;
use crate::state::{ChatItem, Screen};
use serde::{Deserialize, Serialize};

/// Most chats kept, and most items of one chat.
const MAX_CHATS: usize = 30;
const MAX_ITEMS: usize = 200;
const TITLE_CHARS: usize = 44;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedChat {
    pub title: String,
    pub items: Vec<ChatItem>,
}

/// The questions and answers of `chat`, or `None` when nothing was asked.
pub fn keep(chat: &[ChatItem]) -> Option<SavedChat> {
    let items: Vec<ChatItem> =
        chat.iter().filter(|i| matches!(i, ChatItem::User(_)) || matches!(i, ChatItem::Assistant { streaming: false, .. })).take(MAX_ITEMS).cloned().collect();
    let first = items.iter().find_map(|i| if let ChatItem::User(t) = i { Some(t.trim()) } else { None })?;
    let flat: String = first.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() {
        return None;
    }
    let title = if flat.chars().count() > TITLE_CHARS { flat.chars().take(TITLE_CHARS - 1).chain(std::iter::once('…')).collect() } else { flat };
    Some(SavedChat { title, items })
}

impl CraftApp {
    /// Put the chat on the screen into the history (when there is one), leaving the screen's chat alone.
    pub fn archive_chat(&mut self) {
        if let Some(c) = keep(&self.state.chat) {
            self.chats.retain(|o| o != &c);
            self.chats.insert(0, c);
            self.chats.truncate(MAX_CHATS);
        }
    }

    /// Open an earlier chat; the one on the screen goes into the history.
    pub fn open_chat(&mut self, index: usize) {
        if index >= self.chats.len() {
            return;
        }
        let chosen = self.chats.remove(index);
        self.archive_chat();
        if let Some(mut c) = self.conversation.take() {
            c.stop();
        }
        self.state.chat = chosen.items;
        self.state.screen = Screen::Ask;
    }

    /// Forget an earlier chat.
    pub fn delete_chat(&mut self, index: usize) {
        if index < self.chats.len() {
            self.chats.remove(index);
        }
    }

    /// The history as it's saved: the earlier chats and the one on the screen.
    pub fn chats_to_save(&self) -> Vec<SavedChat> {
        let mut all = self.chats.clone();
        if let Some(c) = keep(&self.state.chat) {
            all.retain(|o| o != &c);
            all.insert(0, c);
        }
        all.truncate(MAX_CHATS);
        all
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(q: &str, a: &str) -> Vec<ChatItem> {
        vec![ChatItem::User(q.into()), ChatItem::Status("working".into()), ChatItem::Assistant { text: a.into(), backend: None, streaming: false }]
    }

    #[test]
    fn a_chat_keeps_questions_and_answers_and_is_titled_by_the_first_question() {
        let c = keep(&ask("  How does\nthe % key work?  ", "It divides by 100.")).unwrap();
        assert_eq!(c.title, "How does the % key work?");
        assert_eq!(c.items.len(), 2, "status lines are left out");
        assert!(keep(&[ChatItem::Status("hi".into())]).is_none() && keep(&[]).is_none());
        let long = keep(&ask(&"word ".repeat(40), "x")).unwrap();
        assert_eq!(long.title.chars().count(), TITLE_CHARS);
        assert!(long.title.ends_with('…'));
    }

    #[test]
    fn an_answer_still_streaming_is_not_kept() {
        let mut chat = ask("q", "a");
        chat.push(ChatItem::Assistant { text: "half".into(), backend: None, streaming: true });
        assert_eq!(keep(&chat).unwrap().items.len(), 2);
    }
}
