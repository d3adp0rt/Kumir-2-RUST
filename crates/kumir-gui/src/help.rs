//! Built-in help: topics, full-text search and context lookup.
//!
//! The topic bodies are the Markdown files from `docs/`, embedded at compile
//! time, so the IDE and the repository never drift apart.

use crate::theme::Palette;

pub struct Topic {
    pub id: &'static str,
    pub title: &'static str,
    pub keywords: &'static [&'static str],
    pub source: &'static str,
}

pub const TOPICS: &[Topic] = &[
    Topic {
        id: "language",
        title: "Язык Кумир",
        keywords: &[
            "алг",
            "нач",
            "кон",
            "если",
            "иначе",
            "все",
            "выбор",
            "при",
            "нц",
            "кц",
            "для",
            "пока",
            "раз",
            "ввод",
            "вывод",
            "нс",
            "арг",
            "рез",
            "аргрез",
            "знач",
            "цел",
            "вещ",
            "лит",
            "сим",
            "лог",
            "файл",
            "целтаб",
            "вещтаб",
            "div",
            "mod",
            "sqrt",
            "длин",
            "код",
        ],
        source: include_str!("../../../docs/LANGUAGE.md"),
    },
    Topic {
        id: "actors",
        title: "Исполнители",
        keywords: &[
            "робот",
            "черепаха",
            "чертежник",
            "чертёжник",
            "водолей",
            "кузнечик",
            "рисователь",
            "исполнитель",
            "поле",
            "fil",
            "vod",
            "kz",
            "вперед",
            "назад",
            "закрасить",
            "перо",
            "хвост",
            "кисть",
            "наполни",
            "перелей",
        ],
        source: include_str!("../../../docs/ACTORS.md"),
    },
    Topic {
        id: "ide",
        title: "Интерфейс и клавиши",
        keywords: &[
            "интерфейс",
            "горячие",
            "клавиши",
            "f5",
            "f6",
            "f7",
            "f8",
            "f9",
            "f10",
            "f12",
            "ctrl",
            "тема",
            "шрифт",
            "настройки",
            "android",
            "андроид",
            "справка",
        ],
        source: include_str!("../../../docs/IDE.md"),
    },
    Topic {
        id: "build",
        title: "Сборка и установка",
        keywords: &[
            "сборка",
            "make",
            "cargo",
            "apk",
            "android",
            "windows",
            "linux",
        ],
        source: include_str!("../../../docs/BUILD.md"),
    },
];

pub fn topic(id: &str) -> Option<&'static Topic> {
    TOPICS.iter().find(|t| t.id == id)
}

pub fn default_topic() -> &'static str {
    TOPICS[0].id
}

/// Choose the most relevant topic for a word under the cursor.
pub fn topic_for_word(word: &str) -> Option<&'static str> {
    let w = word.trim().to_lowercase();
    if w.is_empty() {
        return None;
    }
    // Actor commands win over everything else.
    if !kumir_core::actors::commands::lookup_all(&w).is_empty() {
        return Some("actors");
    }
    for topic in TOPICS {
        for keyword in topic.keywords {
            if w == *keyword || w.starts_with(keyword) && keyword.chars().count() >= 3 {
                return Some(topic.id);
            }
        }
    }
    // A known standard function belongs to the language reference.
    if kumir_core::builtins::known_names()
        .iter()
        .any(|n| n.to_lowercase() == w)
    {
        return Some("language");
    }
    None
}

pub struct Hit {
    pub topic: &'static Topic,
    pub line: String,
}

/// Full-text search over the help topics.
pub fn search(query: &str) -> Vec<Hit> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for topic in TOPICS {
        for line in topic.source.lines() {
            if line.to_lowercase().contains(&query) {
                let line = line.trim().trim_start_matches('#').trim().to_string();
                if !line.is_empty() {
                    hits.push(Hit { topic, line });
                }
                if hits.len() > 80 {
                    return hits;
                }
            }
        }
    }
    hits
}

/// Render one topic with a heading and the Markdown body.
pub fn show_topic(ui: &mut egui::Ui, palette: &Palette, id: &str) {
    let Some(topic) = topic(id) else {
        ui.label("Раздел не найден");
        return;
    };
    ui.label(
        egui::RichText::new(topic.title)
            .heading()
            .color(palette.accent),
    );
    ui.separator();
    crate::markdown::show(ui, palette, topic.source);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topics_are_embedded_and_unique() {
        let mut ids = std::collections::HashSet::new();
        for topic in TOPICS {
            assert!(ids.insert(topic.id), "duplicate topic {}", topic.id);
            assert!(!topic.source.trim().is_empty(), "{} is empty", topic.id);
        }
    }

    #[test]
    fn context_lookup() {
        assert_eq!(topic_for_word("наполни A"), Some("actors"));
        assert_eq!(topic_for_word("алг"), Some("language"));
        assert_eq!(topic_for_word("F5"), Some("ide"));
        assert_eq!(topic_for_word("нечто"), None);
    }

    #[test]
    fn search_finds_commands() {
        let hits = search("перелей");
        assert!(!hits.is_empty());
        assert!(hits.iter().any(|h| h.topic.id == "actors"));
    }
}
