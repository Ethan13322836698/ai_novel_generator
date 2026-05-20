use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

use crate::config::AppConfig;

#[derive(Debug)]
pub enum WorkerCmd {
    OptimizeOutline {
        outline: String,
        template_name: String,
    },
    GeneratePlan {
        title: String,
        outline: String,
        count: usize,
        template_name: String,
    },
    GenerateChapter {
        num: usize,
        chapter_title: String,
        brief: String,
        novel_title: String,
        outline: String,
        context: String,
        template_name: String,
        words: usize,
        /// 已选境界体系的格式化说明（空字符串=不使用）
        realm_info: String,
        reduce_ai_traits: bool,
        avoid_famous_names: bool,
    },
    Stop,
}

#[derive(Debug)]
pub enum WorkerMsg {
    Chunk(String),
    OutlineDone(String),
    PlanDone(Vec<(String, String)>),
    ChapterDone(usize, String),
    Error(String),
}

pub struct ApiWorker {
    pub tx_cmd: mpsc::SyncSender<WorkerCmd>,
    pub rx_msg: mpsc::Receiver<WorkerMsg>,
    pub stop_flag: Arc<AtomicBool>,
}

impl ApiWorker {
    pub fn spawn(config: Arc<std::sync::Mutex<AppConfig>>) -> Self {
        let (tx_cmd, rx_cmd) = mpsc::sync_channel::<WorkerCmd>(8);
        let (tx_msg, rx_msg) = mpsc::sync_channel::<WorkerMsg>(256);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_clone = stop_flag.clone();

        std::thread::spawn(move || {
            run_worker(rx_cmd, tx_msg, stop_clone, config);
        });

        Self {
            tx_cmd,
            rx_msg,
            stop_flag,
        }
    }

    pub fn send(&self, cmd: WorkerCmd) {
        self.tx_cmd.try_send(cmd).ok();
    }

    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        self.tx_cmd.try_send(WorkerCmd::Stop).ok();
    }

    pub fn reset_stop(&self) {
        self.stop_flag.store(false, Ordering::SeqCst);
    }
}

fn run_worker(
    rx: mpsc::Receiver<WorkerCmd>,
    tx: mpsc::SyncSender<WorkerMsg>,
    stop: Arc<AtomicBool>,
    config: Arc<std::sync::Mutex<AppConfig>>,
) {
    for cmd in rx {
        stop.store(false, Ordering::SeqCst);
        match cmd {
            WorkerCmd::Stop => {
                stop.store(true, Ordering::SeqCst);
            }
            WorkerCmd::OptimizeOutline { outline, template_name } => {
                let cfg = config.lock().unwrap().clone();
                let tmpl = crate::templates::get_template_by_name(&template_name);
                let (system, prompt) = if let Some(t) = tmpl {
                    let p = t.outline_optimize_prompt.replace("{outline}", &outline);
                    (t.system_prompt, p)
                } else {
                    ("你是专业小说作家。", format!("请优化以下小说大纲，使其更加丰富完善：\n{}", outline))
                };
                let messages = vec![
                    msg("system", system),
                    msg("user", &prompt),
                ];
                match stream_chat(&cfg, messages, &stop, &tx) {
                    Ok(full) => {
                        tx.send(WorkerMsg::OutlineDone(full)).ok();
                    }
                    Err(e) => {
                        tx.send(WorkerMsg::Error(e)).ok();
                    }
                }
            }
            WorkerCmd::GeneratePlan { title, outline, count, template_name } => {
                let cfg = config.lock().unwrap().clone();
                let tmpl = crate::templates::get_template_by_name(&template_name);
                let (system, prompt_template_str) = if let Some(t) = tmpl {
                    (t.system_prompt, t.plan_prompt.to_string())
                } else {
                    ("你是专业小说作家。", "请为小说《{title}》生成{count}章的章节规划，格式：第X章 章节标题\\n本章要点（30字内）\n大纲：{outline}".to_string())
                };
                let prompt = prompt_template_str
                    .replace("{title}", &title)
                    .replace("{count}", &count.to_string())
                    .replace("{outline}", &outline);
                let messages = vec![
                    msg("system", system),
                    msg("user", &prompt),
                ];
                match stream_chat(&cfg, messages, &stop, &tx) {
                    Ok(full) => {
                        let plan = parse_chapter_plan(&full, count);
                        tx.send(WorkerMsg::PlanDone(plan)).ok();
                    }
                    Err(e) => {
                        tx.send(WorkerMsg::Error(e)).ok();
                    }
                }
            }
            WorkerCmd::GenerateChapter {
                num, chapter_title, brief, novel_title, outline,
                context, template_name, words, realm_info,
                reduce_ai_traits, avoid_famous_names,
            } => {
                let cfg = config.lock().unwrap().clone();
                let tmpl = crate::templates::get_template_by_name(&template_name);
                let (system, prompt_template_str) = if let Some(t) = tmpl {
                    (t.system_prompt, t.chapter_prompt.to_string())
                } else {
                    ("你是专业小说作家。", "请创作小说《{title}》第{num}章 {chapter_title}，本章要点：{brief}，前情：{context}，大纲：{outline}，约{words}字。".to_string())
                };
                // 拼接境界体系说明（控制 token：非空才追加）
                let full_outline = if realm_info.is_empty() {
                    outline.clone()
                } else {
                    format!("{}{}", outline, realm_info)
                };
                let mut prompt = prompt_template_str
                    .replace("{title}", &novel_title)
                    .replace("{num}", &num.to_string())
                    .replace("{chapter_title}", &chapter_title)
                    .replace("{brief}", &brief)
                    .replace("{context}", &context)
                    .replace("{outline}", &full_outline)
                    .replace("{words}", &words.to_string());
                if reduce_ai_traits {
                    prompt.push_str("\n\n额外要求：避免明显的 AI 写作特征——少用排比铺陈、固定句式开头、过度比喻；多用具体细节代替形容词；对话符合人物身份不要书面化。");
                }
                if avoid_famous_names {
                    prompt.push_str("\n\n禁用以下主角名（防止与知名小说雷同）：陈平安、韩立、王林、楚天南、萧炎、唐三、洛天、罗峰、林动、叶凡、石昊、孟浩、王腾、李慕白、徐凤年、宁缺、傅红雪。请使用原创姓名。");
                }
                let messages = vec![
                    msg("system", system),
                    msg("user", &prompt),
                ];
                match stream_chat(&cfg, messages, &stop, &tx) {
                    Ok(full) => {
                        tx.send(WorkerMsg::ChapterDone(num, full)).ok();
                    }
                    Err(e) => {
                        tx.send(WorkerMsg::Error(e)).ok();
                    }
                }
            }
        }
    }
}

fn msg(role: &str, content: &str) -> serde_json::Value {
    serde_json::json!({"role": role, "content": content})
}

fn stream_chat(
    cfg: &AppConfig,
    messages: Vec<serde_json::Value>,
    stop: &Arc<AtomicBool>,
    tx: &mpsc::SyncSender<WorkerMsg>,
) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("Content-Type", "application/json".parse().unwrap());
    if !cfg.api_key.is_empty() {
        let auth = format!("Bearer {}", cfg.api_key);
        headers.insert("Authorization", auth.parse().map_err(|e: reqwest::header::InvalidHeaderValue| e.to_string())?);
    }

    let payload = serde_json::json!({
        "model": cfg.model,
        "messages": messages,
        "temperature": cfg.temperature,
        "max_tokens": cfg.max_tokens,
        "stream": true
    });

    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
    let response = client
        .post(&url)
        .headers(headers)
        .json(&payload)
        .send()
        .map_err(|e| format!("连接失败: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(format!("HTTP {} 错误: {}", status, &body[..body.len().min(200)]));
    }

    let reader = BufReader::new(response);
    let mut full_text = String::new();

    for line in reader.lines() {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let line = line.map_err(|e| format!("读取流失败: {}", e))?;
        let data = if let Some(s) = line.strip_prefix("data: ") {
            s.trim()
        } else {
            continue;
        };
        if data == "[DONE]" {
            break;
        }
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
            if let Some(content) = json["choices"][0]["delta"]["content"].as_str() {
                if !content.is_empty() {
                    full_text.push_str(content);
                    tx.send(WorkerMsg::Chunk(content.to_string())).ok();
                }
            }
        }
    }

    Ok(full_text)
}

fn parse_chapter_plan(text: &str, count: usize) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let mut current_title = String::new();
    let mut current_brief = String::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Match patterns like "第1章 xxx" or "第一章 xxx"
        let is_chapter_line = line.contains("章") && (
            line.starts_with("第") || line.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
        );
        if is_chapter_line {
            if !current_title.is_empty() {
                result.push((current_title.clone(), current_brief.trim().to_string()));
                current_brief.clear();
            }
            // Extract title after "章 " or "章："
            let title = if let Some(pos) = line.find("章 ").or_else(|| line.find("章：")) {
                line[pos + "章 ".len()..].trim().to_string()
            } else if let Some(pos) = line.find('章') {
                line[pos + 3..].trim().to_string()
            } else {
                line.to_string()
            };
            current_title = title;
        } else if !current_title.is_empty() {
            if !current_brief.is_empty() {
                current_brief.push(' ');
            }
            current_brief.push_str(line);
        }
    }
    if !current_title.is_empty() {
        result.push((current_title, current_brief.trim().to_string()));
    }

    // If parsing failed or insufficient chapters, generate generic titles
    if result.len() < count.min(3) {
        result.clear();
        for i in 1..=count {
            result.push((format!("第{}章", i), String::new()));
        }
    }

    result.truncate(count);
    while result.len() < count {
        let n = result.len() + 1;
        result.push((format!("第{}章", n), String::new()));
    }

    result
}

pub fn test_connection(cfg: &AppConfig) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("Content-Type", "application/json".parse().unwrap());
    if !cfg.api_key.is_empty() {
        let auth = format!("Bearer {}", cfg.api_key);
        headers.insert("Authorization", auth.parse().map_err(|e: reqwest::header::InvalidHeaderValue| e.to_string())?);
    }

    let payload = serde_json::json!({
        "model": cfg.model,
        "messages": [{"role": "user", "content": "回复：连接成功"}],
        "max_tokens": 20,
        "stream": false
    });

    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .headers(headers)
        .json(&payload)
        .send()
        .map_err(|e| format!("连接失败: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(format!("HTTP {} 错误: {}", status, &body[..body.len().min(300)]));
    }

    let json: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
    let reply = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("连接成功")
        .to_string();
    Ok(reply)
}
