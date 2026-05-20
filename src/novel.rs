use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChapterStatus {
    Pending,
    Generating,
    Done,
    Error(String),
}

impl Default for ChapterStatus {
    fn default() -> Self { Self::Pending }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Chapter {
    pub number: usize,
    pub title: String,
    pub content: String,
    pub brief: String,
    pub status: ChapterStatus,
    pub word_count: usize,
}

impl Chapter {
    pub fn new(number: usize, title: impl Into<String>, brief: impl Into<String>) -> Self {
        Self {
            number,
            title: title.into(),
            brief: brief.into(),
            ..Default::default()
        }
    }

    pub fn update_word_count(&mut self) {
        self.word_count = self.content.chars().filter(|c| !c.is_whitespace()).count();
    }

    pub fn context_tail(&self, chars: usize) -> String {
        let content = &self.content;
        if content.chars().count() <= chars {
            content.clone()
        } else {
            content.chars().rev().take(chars).collect::<String>().chars().rev().collect()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CustomRealm {
    pub order: u32,
    pub name: String,
    pub description: String,
    pub sub_levels: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NovelProject {
    pub title: String,
    pub author: String,
    pub template: String,
    pub outline: String,
    pub optimized_outline: String,
    pub chapters: Vec<Chapter>,
    pub target_chapters: usize,
    pub target_words_per_chapter: usize,
    /// 已选中的境界体系 id 列表（可多选）
    pub selected_realms: Vec<String>,
    /// 用户自定义境界文本（追加到 selected_realms 之后）
    pub custom_realm: String,
    pub reduce_ai_traits: bool,
    pub avoid_famous_names: bool,
    pub custom_realms: Vec<CustomRealm>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Default for NovelProject {
    fn default() -> Self {
        Self {
            title: String::from("我的小说"),
            author: String::new(),
            template: String::from("修仙问道"),
            outline: String::new(),
            optimized_outline: String::new(),
            chapters: Vec::new(),
            target_chapters: 20,
            target_words_per_chapter: 3000,
            selected_realms: Vec::new(),
            custom_realm: String::new(),
            reduce_ai_traits: false,
            avoid_famous_names: false,
            custom_realms: Vec::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

impl NovelProject {
    pub fn total_words(&self) -> usize {
        self.chapters.iter().map(|c| c.word_count).sum()
    }

    pub fn completed_chapters(&self) -> usize {
        self.chapters
            .iter()
            .filter(|c| c.status == ChapterStatus::Done)
            .count()
    }

    pub fn progress(&self) -> f32 {
        if self.chapters.is_empty() {
            0.0
        } else {
            self.completed_chapters() as f32 / self.chapters.len() as f32
        }
    }

    pub fn to_txt(&self) -> String {
        let mut out = format!("{}\n", self.title);
        if !self.author.is_empty() {
            out += &format!("作者：{}\n", self.author);
        }
        out += &"=".repeat(50);
        out += "\n\n";
        if !self.optimized_outline.is_empty() {
            out += "【故事大纲】\n\n";
            out += &self.optimized_outline;
            out += "\n\n";
            out += &"=".repeat(50);
            out += "\n\n";
        }
        for chapter in &self.chapters {
            if chapter.status == ChapterStatus::Done {
                out += &format!("第{}章 {}\n\n", chapter.number, chapter.title);
                out += &chapter.content;
                out += "\n\n";
                out += &"-".repeat(40);
                out += "\n\n";
            }
        }
        out
    }
}
