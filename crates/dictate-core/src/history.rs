//! In-memory drafts. No dictated text is written to disk.
#[derive(Default, Clone)]
pub struct Draft {
    pub original: String,
    pub text: String,
    pub dirty: bool,
}
#[derive(Default)]
pub struct History {
    pub entries: Vec<Draft>,
    pub selected: usize,
}
impl History {
    pub fn begin(&mut self) {
        self.entries.push(Draft::default());
        self.selected = self.entries.len() - 1;
    }
    pub fn captured(&mut self) {
        while self.entries.len() > 10 {
            self.entries.remove(0);
            self.selected = self.selected.saturating_sub(1);
        }
    }
    pub fn current(&self) -> Option<&Draft> {
        self.entries.get(self.selected)
    }
    pub fn is_latest(&self) -> bool {
        !self.entries.is_empty() && self.selected + 1 == self.entries.len()
    }
    pub fn edit(&mut self, text: String) {
        if let Some(draft) = self.entries.get_mut(self.selected)
            && draft.text != text
        {
            draft.text = text;
            draft.dirty = true;
        }
    }
    pub fn transcript(&mut self, text: String) {
        if let Some(draft) = self.entries.last_mut() {
            draft.original = text.clone();
            if !draft.dirty {
                draft.text = text;
            }
        }
    }
    /// Returns false when a late model response was kept out of a user's draft.
    pub fn result(&mut self, original: String, text: String) -> bool {
        if let Some(draft) = self.entries.last_mut() {
            draft.original = original;
            if !draft.dirty {
                draft.text = text;
                return true;
            }
        }
        false
    }
    pub fn select(&mut self, index: usize) {
        if index < self.entries.len() {
            self.selected = index;
        }
    }
    pub fn cancel_recording(&mut self) {
        self.entries.pop();
        self.selected = self.entries.len().saturating_sub(1);
    }
    pub fn labels(&self) -> Vec<String> {
        self.entries
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let title: String = d
                    .text
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(44)
                    .collect();
                format!(
                    "{}. {}",
                    i + 1,
                    if title.is_empty() {
                        "Запись без текста"
                    } else {
                        &title
                    }
                )
            })
            .collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_result_and_retry_preserve_user_edits() {
        let mut h = History::default();
        h.begin();
        h.transcript("исходник".into());
        h.edit("мои правки".into());
        assert!(!h.result("исходник".into(), "ответ модели".into()));
        h.transcript("исходник".into());
        assert_eq!(h.current().unwrap().text, "мои правки");
    }
    #[test]
    fn new_recording_and_selection_keep_previous_drafts() {
        let mut h = History::default();
        h.begin();
        h.result("первый".into(), "Первый.".into());
        h.edit("Первый с правкой.".into());
        h.begin();
        h.cancel_recording();
        assert_eq!(h.current().unwrap().text, "Первый с правкой.");
        for _ in 0..12 {
            h.begin();
            h.captured();
        }
        h.begin();
        h.cancel_recording();
        assert_eq!(h.entries.len(), 10);
        h.select(0);
        assert!(!h.is_latest());
    }
}
