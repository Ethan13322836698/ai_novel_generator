use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};
use egui::{Color32, FontId, Rounding, RichText, Stroke, Vec2, Margin, Rect, pos2, Shape};
use crate::api::{ApiWorker, WorkerCmd, WorkerMsg};
use crate::config::{provider_presets, AppConfig};
use crate::novel::{Chapter, ChapterStatus, CustomRealm, NovelProject};
use crate::realms::{all_realms, build_custom_realms_prompt, build_realm_prompt};
use crate::templates::all_templates;

#[derive(Clone, Copy, PartialEq)]
enum SavePromptAction { Quit, NewProject, OpenProject }

// ═══════════════════════════════════════════════════════
//  主题上下文
// ═══════════════════════════════════════════════════════
#[derive(Clone, Copy)]
struct Th {
    dark: bool,  // M3 暗色 / 亮色
}

impl Th {
    // ── M3 Primary ────────────────────────────────────
    fn primary(self) -> Color32 {
        if self.dark { c(158,202,255) } else { c(0,97,164) }
    }
    fn on_primary(self) -> Color32 {
        if self.dark { c(0,50,88) } else { Color32::WHITE }
    }
    fn primary_container(self) -> Color32 {
        if self.dark { c(0,74,119) } else { c(209,228,255) }
    }
    fn on_primary_container(self) -> Color32 {
        if self.dark { c(209,228,255) } else { c(0,29,54) }
    }

    // ── M3 Secondary ──────────────────────────────────
    fn secondary_container(self) -> Color32 {
        if self.dark { c(59,72,88) } else { c(216,227,248) }
    }
    fn on_secondary_container(self) -> Color32 {
        if self.dark { c(216,227,248) } else { c(16,28,43) }
    }

    // ── M3 Tertiary (success) ─────────────────────────
    fn tertiary_container(self) -> Color32 {
        if self.dark { c(18,75,40) } else { c(196,239,214) }
    }
    fn on_tertiary_container(self) -> Color32 {
        if self.dark { c(196,239,214) } else { c(0,33,14) }
    }

    // ── M3 Error ──────────────────────────────────────
    fn error(self) -> Color32 {
        if self.dark { c(255,180,171) } else { c(186,26,26) }
    }
    fn error_container(self) -> Color32 {
        if self.dark { c(147,0,10) } else { c(255,218,214) }
    }
    fn on_error_container(self) -> Color32 {
        if self.dark { c(255,218,214) } else { c(65,0,2) }
    }

    // ── M3 Surface ────────────────────────────────────
    fn surface(self) -> Color32 {
        if self.dark { c(17,20,24) } else { c(248,249,255) }
    }
    fn on_surface(self) -> Color32 {
        if self.dark { c(225,227,232) } else { c(25,28,32) }
    }
    fn on_surface_variant(self) -> Color32 {
        if self.dark { c(195,199,207) } else { c(67,71,78) }
    }
    fn outline(self) -> Color32 {
        if self.dark { c(141,145,153) } else { c(115,119,127) }
    }
    fn outline_variant(self) -> Color32 {
        if self.dark { c(67,71,78) } else { c(195,199,207) }
    }

    // ── Surface Containers ────────────────────────────
    fn surface_container_lowest(self) -> Color32 {
        if self.dark { c(12,14,19) } else { Color32::WHITE }
    }
    fn surface_container_low(self) -> Color32 {
        if self.dark { c(25,28,32) } else { c(242,243,250) }
    }
    fn surface_container(self) -> Color32 {
        if self.dark { c(29,32,36) } else { c(236,237,244) }
    }
    fn surface_container_high(self) -> Color32 {
        if self.dark { c(40,43,48) } else { c(230,232,239) }
    }
    fn surface_container_highest(self) -> Color32 {
        if self.dark { c(51,54,57) } else { c(224,226,233) }
    }

    // ── State Layers ──────────────────────────────────
    fn hover_state(self, bg: Color32)   -> Color32 { blend(bg, self.on_surface(), 20) }
    fn pressed_state(self, bg: Color32) -> Color32 { blend(bg, self.on_surface(), 31) }
    fn pri_hover(self, bg: Color32)     -> Color32 { blend(bg, self.primary(), 20) }
    fn pri_pressed(self, bg: Color32)   -> Color32 { blend(bg, self.primary(), 31) }
}

fn c(r:u8,g:u8,b:u8) -> Color32 { Color32::from_rgb(r,g,b) }
fn ca(r:u8,g:u8,b:u8,a:u8) -> Color32 { Color32::from_rgba_premultiplied(r,g,b,a) }
fn blend(bg: Color32, fg: Color32, a: u8) -> Color32 {
    let (a,ia) = (a as u16, 255-a as u16);
    c(((bg.r() as u16*ia+fg.r() as u16*a)/255) as u8,
      ((bg.g() as u16*ia+fg.g() as u16*a)/255) as u8,
      ((bg.b() as u16*ia+fg.b() as u16*a)/255) as u8)
}

// M3 Shape Scale
const R4:    Rounding = Rounding::same(4.0);
const R8:    Rounding = Rounding::same(8.0);
const R12:   Rounding = Rounding::same(12.0);
const R16:   Rounding = Rounding::same(16.0);
const R28:   Rounding = Rounding::same(28.0);
const RFULL: Rounding = Rounding::same(9999.0);

// ═══════════════════════════════════════════════════════
//  生成状态
// ═══════════════════════════════════════════════════════
#[derive(Debug, PartialEq, Clone)]
enum GenState {
    Idle, OptimizingOutline, GeneratingPlan, GeneratingChapter(usize), Paused(usize), Done,
}
impl GenState {
    fn is_running(&self) -> bool {
        matches!(self, Self::OptimizingOutline|Self::GeneratingPlan|Self::GeneratingChapter(_))
    }
    fn label(&self, total: usize) -> String {
        match self {
            Self::Idle                 => "就绪".into(),
            Self::OptimizingOutline    => "优化大纲中…".into(),
            Self::GeneratingPlan       => "规划章节中…".into(),
            Self::GeneratingChapter(n) => format!("第 {}/{} 章", n, total),
            Self::Paused(_)            => "已暂停".into(),
            Self::Done                 => "已完成 ✓".into(),
        }
    }
    fn color(&self, th: Th) -> Color32 {
        match self {
            Self::Done      => th.on_tertiary_container(),
            Self::Paused(_) => c(220,130,0),
            Self::Idle      => th.on_surface_variant(),
            _               => th.primary(),
        }
    }
}

// ═══════════════════════════════════════════════════════
//  Toast
// ═══════════════════════════════════════════════════════
#[derive(Clone,Copy,PartialEq)] enum ToastKind { Info, Ok, Err }
struct Toast { msg: String, kind: ToastKind, born: Instant }
impl Toast {
    fn new(msg: impl Into<String>, k: ToastKind) -> Self { Self { msg:msg.into(), kind:k, born:Instant::now() } }
    fn alive(&self) -> bool { self.born.elapsed().as_secs_f32() < 3.5 }
    fn alpha(&self) -> f32 {
        let t = self.born.elapsed().as_secs_f32();
        if t<0.2 {t/0.2} else if t>3.0 {1.0-(t-3.0)/0.5} else {1.0}
    }
    fn bg(&self, th: Th) -> Color32 { match self.kind {
        ToastKind::Ok  => th.tertiary_container(),
        ToastKind::Err => th.error_container(),
        _              => th.surface_container_high(),
    }}
    fn fg(&self, th: Th) -> Color32 { match self.kind {
        ToastKind::Ok  => th.on_tertiary_container(),
        ToastKind::Err => th.on_error_container(),
        _              => th.on_surface_variant(),
    }}
}

#[derive(Clone,PartialEq)] enum BgSource { None, Local(std::path::PathBuf), Url(String) }

// ═══════════════════════════════════════════════════════
//  App 状态
// ═══════════════════════════════════════════════════════
pub struct NovelApp {
    project:  NovelProject,
    config:   Arc<Mutex<AppConfig>>,
    selected_chapter: Option<usize>,
    show_settings: bool,
    show_about:    bool,
    pick_local_bg: bool,

    title_buf: String,
    count_buf: String,
    words_buf: String,

    s_provider: String, s_api_key: String, s_base_url: String,
    s_model: String, s_model_input: String,
    s_temperature: f32, s_max_tokens: String, s_font_size: f32,
    s_test_result: Option<(bool, String)>,

    // 外观
    dark_mode:   bool,

    // 背景图（支持 GIF 多帧）
    bg_frames:        Vec<egui::TextureHandle>,
    bg_delays_ms:     Vec<u32>,
    bg_frame_idx:     usize,
    bg_last_advance:  Option<Instant>,
    bg_blurred:       Option<egui::TextureHandle>,
    bg_source:    BgSource,
    bg_url_input: String,
    bg_loading:   bool,
    bg_rx:        Option<mpsc::Receiver<Result<Vec<u8>, String>>>,

    worker:       ApiWorker,
    gen_state:    GenState,
    streaming:    Option<usize>,
    current_file: Option<std::path::PathBuf>,
    toasts:       Vec<Toast>,

    pub dirty: bool,
    show_save_prompt: bool,
    save_prompt_action: SavePromptAction,

    show_wizard: bool,
    wizard_page: u8,
    wizard_proj: NovelProject,
    wizard_realm_dialog: Option<CustomRealm>,
    wizard_count_buf: String,
    wizard_words_buf: String,
}

impl NovelApp {
    pub fn new() -> Self {
        let config = Arc::new(Mutex::new(AppConfig::load()));
        let worker = ApiWorker::spawn(config.clone());
        let cfg    = config.lock().unwrap().clone();
        let project = NovelProject::default();
        let mut app = Self {
            title_buf: project.title.clone(),
            count_buf: project.target_chapters.to_string(),
            words_buf: project.target_words_per_chapter.to_string(),
            s_provider:    cfg.provider.clone(), s_api_key: cfg.api_key.clone(),
            s_base_url:    cfg.base_url.clone(), s_model: cfg.model.clone(),
            s_model_input: cfg.model.clone(),    s_temperature: cfg.temperature,
            s_max_tokens:  cfg.max_tokens.to_string(), s_font_size: cfg.font_size,
            s_test_result: None,
            dark_mode: false,
            bg_frames: Vec::new(), bg_delays_ms: Vec::new(),
            bg_frame_idx: 0, bg_last_advance: None, bg_blurred: None,
            bg_source: BgSource::None,
            bg_url_input: String::new(), bg_loading: false, bg_rx: None,
            pick_local_bg: false,
            project, config,
            selected_chapter: None, show_settings: false, show_about: false,
            worker, gen_state: GenState::Idle, streaming: None,
            current_file: None, toasts: Vec::new(),
            dirty: false,
            show_save_prompt: false,
            save_prompt_action: SavePromptAction::Quit,
            show_wizard: false,
            wizard_page: 0,
            wizard_proj: NovelProject::default(),
            wizard_realm_dialog: None,
            wizard_count_buf: String::new(),
            wizard_words_buf: String::new(),
        };
        if !cfg.setup_done { app.show_settings = true; }
        app
    }

    fn th(&self) -> Th {
        Th { dark: self.dark_mode }
    }
    fn toast(&mut self, msg: impl Into<String>, k: ToastKind) { self.toasts.push(Toast::new(msg, k)); }
    fn safe_idx(&self) -> Option<usize> { self.selected_chapter.filter(|&i| i < self.project.chapters.len()) }

    // ── 背景图加载 ─────────────────────────────────────
    fn load_bg_bytes(&mut self, ctx: &egui::Context, bytes: Vec<u8>) {
        // 探测 GIF：以 "GIF8" 头判定
        let is_gif = bytes.len() >= 4 && &bytes[..4] == b"GIF8";
        if is_gif {
            match decode_gif(&bytes) {
                Ok((frames, delays)) => {
                    let mut textures = Vec::with_capacity(frames.len());
                    for (i, ci) in frames.iter().enumerate() {
                        textures.push(ctx.load_texture(format!("bg_f{}", i), ci.clone(), egui::TextureOptions::LINEAR));
                    }
                    // 模糊版以首帧为准
                    if let Some(first_blur) = frames.first().and_then(|_| decode_blurred(&bytes).ok()) {
                        self.bg_blurred = Some(ctx.load_texture("bg_blur", first_blur, egui::TextureOptions::LINEAR));
                    }
                    self.bg_frames = textures;
                    self.bg_delays_ms = delays;
                    self.bg_frame_idx = 0;
                    self.bg_last_advance = Some(Instant::now());
                    self.toast("背景已加载 ✓", ToastKind::Ok);
                }
                Err(e) => self.toast(format!("GIF 解析失败: {}", e), ToastKind::Err),
            }
        } else {
            match decode_image(&bytes) {
                Ok(ci) => {
                    let tex = ctx.load_texture("bg", ci, egui::TextureOptions::LINEAR);
                    self.bg_frames = vec![tex];
                    self.bg_delays_ms = vec![0];
                    self.bg_frame_idx = 0;
                    self.bg_last_advance = None;
                    if let Ok(blur) = decode_blurred(&bytes) {
                        self.bg_blurred = Some(ctx.load_texture("bg_blur", blur, egui::TextureOptions::LINEAR));
                    }
                    self.toast("背景已加载 ✓", ToastKind::Ok);
                }
                Err(e) => self.toast(format!("图片解析失败: {}", e), ToastKind::Err),
            }
        }
        self.bg_loading = false;
    }
    fn clear_bg(&mut self) {
        self.bg_frames.clear();
        self.bg_delays_ms.clear();
        self.bg_frame_idx = 0;
        self.bg_last_advance = None;
        self.bg_blurred = None;
        self.bg_source = BgSource::None;
    }
    fn load_bg_local(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("图片", &["png","jpg","jpeg","webp","gif"]).pick_file()
        {
            match std::fs::read(&path) {
                Ok(bytes) => { self.bg_source = BgSource::Local(path); self.load_bg_bytes(ctx, bytes); }
                Err(e)    => self.toast(format!("读取失败: {}", e), ToastKind::Err),
            }
        }
    }
    fn load_bg_url(&mut self) {
        let url = self.bg_url_input.trim().to_string();
        if url.is_empty() { self.toast("请输入图片链接", ToastKind::Err); return; }
        let (tx, rx) = mpsc::channel();
        let u2 = url.clone();
        std::thread::spawn(move || {
            let r = reqwest::blocking::get(&u2)
                .and_then(|r| r.bytes()).map(|b| b.to_vec()).map_err(|e| e.to_string());
            tx.send(r).ok();
        });
        self.bg_source = BgSource::Url(url); self.bg_rx = Some(rx);
        self.bg_loading = true; self.toast("正在下载背景图…", ToastKind::Info);
    }
    fn poll_bg(&mut self, ctx: &egui::Context) {
        let data = if let Some(rx) = &self.bg_rx { rx.try_recv().ok() } else { None };
        if let Some(result) = data {
            self.bg_rx = None;
            match result {
                Ok(b)  => self.load_bg_bytes(ctx, b),
                Err(e) => { self.toast(format!("下载失败: {}", e), ToastKind::Err); self.bg_loading = false; }
            }
        }
    }

    fn pump(&mut self) {
        loop { match self.worker.rx_msg.try_recv() { Ok(m) => self.handle_msg(m), Err(_) => break } }
    }
    fn handle_msg(&mut self, msg: WorkerMsg) {
        match msg {
            WorkerMsg::Chunk(t) => {
                if let Some(i) = self.streaming {
                    if i < self.project.chapters.len() {
                        self.project.chapters[i].content.push_str(&t);
                        self.project.chapters[i].update_word_count();
                        if self.selected_chapter.is_none() { self.selected_chapter = Some(i); }
                    }
                }
            }
            WorkerMsg::OutlineDone(t) => {
                self.project.optimized_outline = t;
                self.gen_state = GenState::Idle;
                self.toast("大纲优化完成 ✓", ToastKind::Ok);
            }
            WorkerMsg::PlanDone(plan) => {
                if plan.is_empty() { self.gen_state = GenState::Idle; self.toast("章节规划失败", ToastKind::Err); return; }
                self.project.chapters = plan.into_iter().enumerate().map(|(i,(t,b))| Chapter::new(i+1,t,b)).collect();
                self.selected_chapter = Some(0); self.launch(0);
            }
            WorkerMsg::ChapterDone(num, content) => {
                let idx = num.saturating_sub(1);
                if idx < self.project.chapters.len() {
                    self.project.chapters[idx].content = content;
                    self.project.chapters[idx].update_word_count();
                    self.project.chapters[idx].status = ChapterStatus::Done;
                }
                if !matches!(self.gen_state, GenState::Paused(_)) {
                    let next = idx+1;
                    if next < self.project.chapters.len() { self.launch(next); }
                    else {
                        self.gen_state = GenState::Done; self.streaming = None;
                        self.toast(format!("全 {} 章完成，共 {} 字",
                            self.project.chapters.len(), fmt_num(self.project.total_words())), ToastKind::Ok);
                        self.auto_save();
                    }
                }
            }
            WorkerMsg::Error(e) => {
                if let Some(i) = self.streaming {
                    if i < self.project.chapters.len() { self.project.chapters[i].status = ChapterStatus::Error(e.clone()); }
                }
                self.gen_state = GenState::Idle; self.streaming = None;
                self.toast(format!("错误：{}", e), ToastKind::Err);
            }
        }
    }
    fn launch(&mut self, idx: usize) {
        if idx >= self.project.chapters.len() { return; }
        let num   = self.project.chapters[idx].number;
        let title = self.project.chapters[idx].title.clone();
        let brief = self.project.chapters[idx].brief.clone();
        let context = if idx>0 { self.project.chapters[idx-1].context_tail(400) } else { String::new() };
        let raw = if self.project.optimized_outline.is_empty() { &self.project.outline } else { &self.project.optimized_outline };
        let outline = truncate_chars(raw, 1200);
        let realm_info = {
            let mut info = build_realm_prompt(&self.project.selected_realms);
            let custom = self.project.custom_realm.trim().to_string();
            if !custom.is_empty() {
                if info.is_empty() { info = format!("\n\n【境界体系】\n▸ 自定义：{}\n", custom); }
                else { info.push_str(&format!("\n▸ 自定义：{}", custom)); }
            }
            info.push_str(&build_custom_realms_prompt(&self.project.custom_realms));
            info
        };
        self.project.chapters[idx].status = ChapterStatus::Generating;
        self.project.chapters[idx].content.clear();
        self.streaming = Some(idx); self.gen_state = GenState::GeneratingChapter(idx+1);
        self.selected_chapter = Some(idx); self.worker.reset_stop();
        self.worker.send(WorkerCmd::GenerateChapter {
            num, chapter_title: title, brief,
            novel_title: self.project.title.clone(), outline, context,
            template_name: self.project.template.clone(),
            extra_templates: self.project.extra_templates.clone(),
            words: self.project.target_words_per_chapter, realm_info,
            reduce_ai_traits: self.project.reduce_ai_traits,
            avoid_famous_names: self.project.avoid_famous_names,
            custom_template_desc: self.project.custom_template_desc.clone(),
        });
    }
    fn begin_gen(&mut self) {
        if self.project.title.trim().is_empty() { self.toast("请填写小说标题", ToastKind::Err); return; }
        let outline = if self.project.optimized_outline.is_empty() { self.project.outline.clone() }
                      else { self.project.optimized_outline.clone() };
        if outline.trim().is_empty() { self.toast("请先填写故事大纲", ToastKind::Err); return; }

        // 已有章节：找第一个「未完成或写了一半」的章节续写
        if !self.project.chapters.is_empty() {
            // 半成品阈值：字数低于目标的 60% 视为未写完，需重新生成
            let threshold = (self.project.target_words_per_chapter as f32 * 0.6) as usize;
            let next = self.project.chapters.iter().position(|c| {
                !matches!(c.status, ChapterStatus::Done) || c.word_count < threshold
            });
            match next {
                Some(idx) => {
                    self.worker.reset_stop();
                    let ch = &self.project.chapters[idx];
                    let msg = if matches!(ch.status, ChapterStatus::Done) && ch.word_count < threshold {
                        format!("第 {} 章字数偏少，重新生成…", idx + 1)
                    } else {
                        format!("从第 {} 章续写…", idx + 1)
                    };
                    self.toast(msg, ToastKind::Info);
                    self.launch(idx);
                }
                None => self.toast("全部章节已生成完成", ToastKind::Info),
            }
            return;
        }

        // 全新生成：规划 + 逐章
        self.worker.reset_stop();
        self.gen_state = GenState::GeneratingPlan;
        self.selected_chapter = None;
        self.worker.send(WorkerCmd::GeneratePlan {
            title: self.project.title.clone(),
            outline: truncate_chars(&outline, 1500),
            count: self.project.target_chapters,
            template_name: self.project.template.clone(),
            extra_templates: self.project.extra_templates.clone(),
            custom_template_desc: self.project.custom_template_desc.clone(),
        });
        self.toast("正在规划章节…", ToastKind::Info);
    }
    fn auto_save(&mut self) {
        if self.config.lock().unwrap().auto_save {
            if let Some(path) = self.current_file.clone() {
                if let Ok(j) = serde_json::to_string_pretty(&self.project) { std::fs::write(path,j).ok(); }
            }
        }
    }
    fn do_save(&mut self) {
        let path = self.current_file.clone().unwrap_or_else(|| {
            rfd::FileDialog::new().add_filter("小说项目",&["json"])
                .set_file_name(&format!("{}.json",self.project.title))
                .save_file().unwrap_or_default()
        });
        if path.as_os_str().is_empty() { return; }
        if let Ok(j) = serde_json::to_string_pretty(&self.project) {
            if std::fs::write(&path,j).is_ok() { self.current_file=Some(path); self.dirty=false; self.toast("已保存 ✓",ToastKind::Ok); }
        }
    }
    fn do_open(&mut self) {
        if let Some(path) = rfd::FileDialog::new().add_filter("小说项目",&["json"]).pick_file() {
            let data = match std::fs::read_to_string(&path) {
                Ok(d) => d,
                Err(e) => { self.toast(format!("读取失败：{}", e), ToastKind::Err); return; }
            };
            match serde_json::from_str::<NovelProject>(&data) {
                Ok(mut proj) => {
                    // 写到一半被中断的章节：清空残稿，标 Pending 让续写时重新生成
                    for ch in &mut proj.chapters {
                        if matches!(ch.status, ChapterStatus::Generating) {
                            ch.content.clear();
                            ch.status = ChapterStatus::Pending;
                        }
                        ch.update_word_count();
                    }
                    let n_ch = proj.chapters.len();
                    self.title_buf = proj.title.clone();
                    self.count_buf = proj.target_chapters.to_string();
                    self.words_buf = proj.target_words_per_chapter.to_string();
                    self.project = proj;
                    self.current_file = Some(path);
                    self.gen_state = GenState::Idle;
                    self.dirty = false;
                    self.selected_chapter = if n_ch > 0 { Some(0) } else { None };
                    self.toast(format!("已加载 ✓  ({} 章)", n_ch), ToastKind::Ok);
                }
                Err(e) => self.toast(format!("解析失败：{}", e), ToastKind::Err),
            }
        }
    }

    fn perform_save_prompt_action(&mut self, ctx: &egui::Context) {
        match self.save_prompt_action {
            SavePromptAction::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            SavePromptAction::NewProject => self.open_wizard(),
            SavePromptAction::OpenProject => self.do_open(),
        }
    }

    fn open_wizard(&mut self) {
        self.wizard_proj = NovelProject::default();
        self.wizard_count_buf = self.wizard_proj.target_chapters.to_string();
        self.wizard_words_buf = self.wizard_proj.target_words_per_chapter.to_string();
        self.wizard_page = 0;
        self.wizard_realm_dialog = None;
        self.show_wizard = true;
    }

    fn finish_wizard(&mut self) {
        self.project = self.wizard_proj.clone();
        self.title_buf = self.project.title.clone();
        self.count_buf = self.project.target_chapters.to_string();
        self.words_buf = self.project.target_words_per_chapter.to_string();
        self.selected_chapter = None;
        self.gen_state = GenState::Idle;
        self.current_file = None;
        self.show_wizard = false;
        self.dirty = true;
    }
}

// ═══════════════════════════════════════════════════════
//  绘制工具库
// ═══════════════════════════════════════════════════════

/// M3 Elevation Level-1 投影
fn shadow1_shape(rect: Rect, r: Rounding) -> Shape {
    Shape::Vec(vec![
        Shape::rect_filled(rect.translate(Vec2::new(0.0,1.0)).expand(0.5), r, ca(0,0,0,28)),
        Shape::rect_filled(rect.translate(Vec2::new(0.0,2.5)).expand(1.0), r, ca(0,0,0,14)),
    ])
}

fn fnt(s: f32) -> FontId { FontId::proportional(s) }

// ── M3 Filled Button ──────────────────────────────────
fn btn_filled(ui: &mut egui::Ui, label: &str, enabled: bool, th: Th) -> egui::Response {
    let gal = ui.fonts(|f| f.layout_no_wrap(label.into(), fnt(13.0), Color32::WHITE));
    let desired = Vec2::new((gal.size().x + 32.0).max(44.0).min(220.0), 36.0);
    let (rect, resp) = ui.allocate_exact_size(desired, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        if enabled { ui.painter().add(shadow1_shape(rect, RFULL)); }
        let bg = if !enabled { blend(th.surface_container(), th.on_surface(), 31) }
            else if resp.is_pointer_button_down_on() { blend(th.primary(), th.on_primary(), 31) }
            else if resp.hovered() { blend(th.primary(), th.on_primary(), 20) }
            else { th.primary() };
        ui.painter().rect_filled(rect, RFULL, bg);
        let fg = if enabled { th.on_primary() } else { ca(th.on_surface().r(),th.on_surface().g(),th.on_surface().b(),97) };
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, fnt(13.0), fg);
    }
    if enabled && resp.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
    resp
}

// ── M3 Tonal Button ───────────────────────────────────
fn btn_tonal(ui: &mut egui::Ui, label: &str, enabled: bool, th: Th) -> egui::Response {
    let gal = ui.fonts(|f| f.layout_no_wrap(label.into(), fnt(13.0), Color32::WHITE));
    let desired = Vec2::new((gal.size().x + 32.0).max(44.0).min(220.0), 36.0);
    let (rect, resp) = ui.allocate_exact_size(desired, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let bg = if !enabled { blend(th.surface_container(), th.on_surface(), 31) }
            else if resp.is_pointer_button_down_on() { th.pri_pressed(th.secondary_container()) }
            else if resp.hovered() { th.pri_hover(th.secondary_container()) }
            else { th.secondary_container() };
        ui.painter().rect_filled(rect, RFULL, bg);
        let fg = if enabled { th.on_secondary_container() } else { ca(th.on_surface().r(),th.on_surface().g(),th.on_surface().b(),97) };
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, fnt(13.0), fg);
    }
    if enabled && resp.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
    resp
}

// ── M3 Outlined Button ────────────────────────────────
fn btn_outlined(ui: &mut egui::Ui, label: &str, enabled: bool, th: Th) -> egui::Response {
    let gal = ui.fonts(|f| f.layout_no_wrap(label.into(), fnt(13.0), Color32::WHITE));
    let desired = Vec2::new((gal.size().x + 32.0).max(44.0).min(220.0), 36.0);
    let (rect, resp) = ui.allocate_exact_size(desired, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let border = if enabled { th.outline() } else { ca(th.on_surface().r(),th.on_surface().g(),th.on_surface().b(),31) };
        let bg = if resp.is_pointer_button_down_on() { th.pri_pressed(th.surface_container()) }
            else if resp.hovered() { th.pri_hover(th.surface_container()) }
            else { th.surface_container() };
        ui.painter().rect_filled(rect, RFULL, bg);
        ui.painter().rect_stroke(rect, RFULL, Stroke::new(1.0, border));
        let fg = if enabled { th.primary() } else { ca(th.on_surface().r(),th.on_surface().g(),th.on_surface().b(),97) };
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, fnt(13.0), fg);
    }
    if enabled && resp.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
    resp
}

// ── Text Button ───────────────────────────────────────
fn btn_text(ui: &mut egui::Ui, label: &str, color: Color32) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(label).color(color).size(13.0))
        .fill(Color32::TRANSPARENT).rounding(RFULL).min_size(Vec2::new(0.0,36.0)))
}

// ── Icon Button ───────────────────────────────────────
fn btn_icon(ui: &mut egui::Ui, label: &str, th: Th) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(40.0), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let bg = if resp.is_pointer_button_down_on() { th.pressed_state(th.surface_container()) }
            else if resp.hovered() { th.hover_state(th.surface_container()) }
            else { Color32::TRANSPARENT };
        ui.painter().rect_filled(rect, RFULL, bg);
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, fnt(14.0), th.on_surface_variant());
    }
    resp
}

// ── Filter Chip ───────────────────────────────────────
fn chip(ui: &mut egui::Ui, label: &str, selected: bool, th: Th) -> egui::Response {
    let gal = ui.fonts(|f| f.layout_no_wrap(label.into(), fnt(13.0), Color32::WHITE));
    let desired = Vec2::new(gal.size().x + 24.0, 32.0);
    let (rect, resp) = ui.allocate_exact_size(desired, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let (bg, fg, border) = if selected {
            let bg = if resp.is_pointer_button_down_on() { th.pri_pressed(th.secondary_container()) }
                else if resp.hovered() { th.pri_hover(th.secondary_container()) }
                else { th.secondary_container() };
            (bg, th.on_secondary_container(), Stroke::NONE)
        } else {
            let bg = if resp.is_pointer_button_down_on() { th.pressed_state(th.surface_container()) }
                else if resp.hovered() { th.hover_state(th.surface_container()) }
                else { th.surface_container() };
            (bg, th.on_surface_variant(), Stroke::new(1.0, th.outline_variant()))
        };
        ui.painter().rect_filled(rect, RFULL, bg);
        if border != Stroke::NONE { ui.painter().rect_stroke(rect, RFULL, border); }
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, fnt(13.0), fg);
    }
    if resp.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
    resp
}

/// Elevated Card（M3）
fn card<R>(ui: &mut egui::Ui, th: Th, r: Rounding, pad: Margin, f: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<R> {
    let slot = ui.painter().add(Shape::Noop);
    let inner = egui::Frame::none().fill(th.surface_container_low()).rounding(r).inner_margin(pad).show(ui, |ui| f(ui));
    ui.painter().set(slot, shadow1_shape(inner.response.rect, r));
    inner
}

/// Filled Card（M3）
fn card_filled<R>(ui: &mut egui::Ui, th: Th, r: Rounding, pad: Margin, f: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<R> {
    egui::Frame::none().fill(th.surface_container_highest()).rounding(r).inner_margin(pad).show(ui, |ui| f(ui))
}

fn divider_h(ui: &mut egui::Ui, th: Th) {
    ui.add_space(6.0);
    let (rect,_) = ui.allocate_exact_size(Vec2::new(ui.available_width(),1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, Rounding::ZERO, th.outline_variant());
    ui.add_space(6.0);
}
fn divider_v(ui: &mut egui::Ui, th: Th) {
    let (rect,_) = ui.allocate_exact_size(Vec2::new(1.0,20.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, Rounding::ZERO, th.outline_variant());
}

// ═══════════════════════════════════════════════════════
//  eframe::App
// ═══════════════════════════════════════════════════════
impl eframe::App for NovelApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pump();
        self.poll_bg(ctx);
        self.toasts.retain(|t| t.alive());
        if let Some(i) = self.selected_chapter {
            if i >= self.project.chapters.len() {
                self.selected_chapter = if self.project.chapters.is_empty() { None } else { Some(self.project.chapters.len()-1) };
            }
        }
        if self.gen_state.is_running() || self.bg_loading { ctx.request_repaint(); }

        let th = self.th();
        let font_sz = self.config.lock().map(|c| c.font_size).unwrap_or(15.0);

        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S)) { self.do_save(); }
        if self.pick_local_bg { self.pick_local_bg = false; self.load_bg_local(ctx); }

        apply_theme(ctx, th);

        // 背景图层（含 GIF 动画）
        if !self.bg_frames.is_empty() {
            // 推进动画帧
            if self.bg_frames.len() > 1 {
                let now = Instant::now();
                let last = self.bg_last_advance.unwrap_or(now);
                let delay = self.bg_delays_ms.get(self.bg_frame_idx).copied().unwrap_or(100).max(20);
                if now.duration_since(last) >= Duration::from_millis(delay as u64) {
                    self.bg_frame_idx = (self.bg_frame_idx + 1) % self.bg_frames.len();
                    self.bg_last_advance = Some(now);
                }
                ctx.request_repaint_after(Duration::from_millis(delay as u64));
            }
            let tex = &self.bg_frames[self.bg_frame_idx.min(self.bg_frames.len()-1)];
            let screen = ctx.screen_rect();
            let p = ctx.layer_painter(egui::LayerId::background());
            p.image(tex.id(), screen, Rect::from_min_max(pos2(0.0,0.0), pos2(1.0,1.0)), Color32::WHITE);
            let overlay_alpha = if self.dark_mode { 100u8 } else { 60u8 };
            p.rect_filled(screen, Rounding::ZERO, ca(0,0,12, overlay_alpha));
        }

        if ctx.input(|i| i.viewport().close_requested()) {
            if self.dirty {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.show_save_prompt = true;
                self.save_prompt_action = SavePromptAction::Quit;
            }
        }

        self.draw_top_bar(ctx, th);
        self.draw_center(ctx, th, font_sz);
        self.draw_toasts(ctx, th);
        if self.show_settings { self.draw_settings(ctx, th); }
        if self.show_about    { self.draw_about(ctx, th); }
        if self.show_save_prompt { self.draw_save_prompt(ctx, th); }
    }
}

fn apply_theme(ctx: &egui::Context, th: Th) {
    let mut vis = ctx.style().visuals.clone();
    vis.dark_mode = th.dark;
    vis.panel_fill       = th.surface();
    vis.window_fill      = th.surface_container_low();
    vis.extreme_bg_color = th.surface_container_lowest();
    vis.window_rounding  = R28;
    vis.window_stroke    = Stroke::new(1.0, th.outline_variant());
    vis.selection.bg_fill  = blend(th.primary_container(), th.primary(), 60);
    vis.selection.stroke   = Stroke::new(1.0, th.primary());
    vis.override_text_color = Some(th.on_surface());
    let mk = |bg: Color32, fg: Color32, r: Rounding| egui::style::WidgetVisuals {
        bg_fill:bg, weak_bg_fill:bg,
        bg_stroke: Stroke::new(1.0, th.outline_variant()),
        fg_stroke: Stroke::new(1.0, fg), rounding:r, expansion:0.0,
    };
    vis.widgets.noninteractive = mk(th.surface_container(),      th.on_surface_variant(), R8);
    vis.widgets.inactive       = mk(th.surface_container_high(), th.on_surface_variant(), R8);
    vis.widgets.hovered        = mk(th.hover_state(th.surface_container_high()), th.on_surface(), R8);
    vis.widgets.active         = mk(th.secondary_container(),    th.on_secondary_container(), R8);
    vis.widgets.open           = mk(th.surface_container_high(), th.on_surface(), R8);
    ctx.set_visuals(vis);
}

// ═══════════════════════════════════════════════════════
//  Top App Bar
// ═══════════════════════════════════════════════════════
impl NovelApp {
    fn draw_top_bar(&mut self, ctx: &egui::Context, th: Th) {
        let running = self.gen_state.is_running();
        let paused  = matches!(self.gen_state, GenState::Paused(_));
        let has_chs = !self.project.chapters.is_empty();
        let compact = ctx.screen_rect().width() < 1100.0;

        egui::TopBottomPanel::top("top_bar").exact_height(56.0)
            .frame(egui::Frame::none().fill(th.surface_container())
                .stroke(Stroke::new(1.0, th.outline_variant()))
                .inner_margin(Margin::symmetric(8.0,8.0)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(4.0);
                    ui.label(RichText::new("✍").size(20.0));
                    ui.add_space(6.0);
                    ui.label(RichText::new("AI 小说").size(17.0).color(th.on_surface()).strong());
                    ui.add_space(8.0); divider_v(ui,th); ui.add_space(8.0);

                    // 文件操作
                    if btn_icon(ui,"📄",th).on_hover_text("新建").clicked() {
                        if self.dirty {
                            self.show_save_prompt = true;
                            self.save_prompt_action = SavePromptAction::NewProject;
                        } else {
                            self.open_wizard();
                        }
                    }
                    if btn_icon(ui,"📂",th).on_hover_text("打开").clicked() {
                        if self.dirty {
                            self.show_save_prompt = true;
                            self.save_prompt_action = SavePromptAction::OpenProject;
                        } else {
                            self.do_open();
                        }
                    }
                    if btn_icon(ui,"💾",th).on_hover_text("保存 ⌘S").clicked() { self.do_save(); }
                    ui.add_space(4.0); divider_v(ui,th); ui.add_space(4.0);

                    // 生成控制
                    if running {
                        ui.add(egui::Spinner::new().color(th.primary()).size(14.0));
                        ui.add_space(8.0);
                        ui.label(RichText::new(self.gen_state.label(self.project.chapters.len())).size(13.0).color(th.primary()));
                        ui.add_space(8.0);
                    } else {
                        let threshold = (self.project.target_words_per_chapter as f32 * 0.6) as usize;
                        let has_pending = self.project.chapters.iter()
                            .any(|c| !matches!(c.status, ChapterStatus::Done) || c.word_count < threshold);
                        let gen_label = if compact { "▶" }
                        else if self.project.chapters.is_empty() { "▶  开始生成" }
                        else if has_pending { "▶  续写" }
                        else { "✓  已完成" };
                        if btn_filled(ui, gen_label, !paused && (self.project.chapters.is_empty() || has_pending), th).clicked() { self.begin_gen(); }
                        ui.add_space(6.0);
                    }
                    let (pl,pe) = if paused {(if compact {"▶"} else {"▶ 继续"},true)} else {(if compact {"⏸"} else {"⏸ 暂停"},running)};
                    if btn_tonal(ui,pl,pe,th).clicked() {
                        if paused { if let GenState::Paused(n)=self.gen_state.clone() { self.gen_state=GenState::Idle; self.launch(n); } }
                        else if let GenState::GeneratingChapter(n)=self.gen_state.clone() { self.worker.stop(); self.gen_state=GenState::Paused(n.saturating_sub(1)); }
                    }
                    ui.add_space(6.0);
                    if btn_outlined(ui, if compact {"⏹"} else {"⏹ 停止"},running||paused,th).clicked() {
                        self.worker.stop(); self.gen_state=GenState::Idle;
                        if let Some(i)=self.streaming {
                            if i<self.project.chapters.len() && self.project.chapters[i].status==ChapterStatus::Generating {
                                self.project.chapters[i].status=ChapterStatus::Pending;
                            }
                        }
                        self.streaming=None; self.toast("已停止",ToastKind::Info);
                    }

                    // 右侧
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if btn_icon(ui,"⚙",th).on_hover_text("设置").clicked() { self.show_settings=true; }
                        if btn_icon(ui,"ℹ",th).on_hover_text("关于").clicked() { self.show_about=true; }
                        // 暗色切换
                        if btn_icon(ui, if th.dark {"☀"} else {"🌙"}, th)
                            .on_hover_text(if th.dark {"切换亮色"} else {"切换暗色"}).clicked()
                        { self.dark_mode = !self.dark_mode; }

                        if has_chs {
                            ui.add_space(4.0); divider_v(ui,th); ui.add_space(8.0);
                            let done=self.project.completed_chapters(); let total=self.project.chapters.len();
                            let prog=done as f32/total as f32;
                            ui.label(RichText::new(format!("{}/{}",done,total)).size(12.0).color(th.on_surface_variant()));
                            ui.add_space(6.0);
                            let (br,_)=ui.allocate_exact_size(Vec2::new(68.0,4.0), egui::Sense::hover());
                            ui.painter().rect_filled(br, RFULL, th.surface_container_highest());
                            if prog>0.0 { let fr=Rect::from_min_size(br.min,Vec2::new(br.width()*prog,4.0)); ui.painter().rect_filled(fr,RFULL,self.gen_state.color(th)); }
                            ui.add_space(4.0); divider_v(ui,th); ui.add_space(4.0);
                            if btn_tonal(ui, if compact {"📤"} else {"📤 导出"},true,th).clicked() {
                                if let Some(path)=rfd::FileDialog::new().add_filter("文本",&["txt"]).set_file_name(&format!("{}.txt",self.project.title)).save_file() {
                                    if std::fs::write(&path,self.project.to_txt().as_bytes()).is_ok() { self.toast("TXT 已导出 ✓",ToastKind::Ok); }
                                }
                            }
                        }
                    });
                });
            });
    }
}

// ═══════════════════════════════════════════════════════
//  Navigation Drawer
// ═══════════════════════════════════════════════════════
impl NovelApp {
    fn draw_nav_drawer(&mut self, ctx: &egui::Context, th: Th) {
        egui::SidePanel::left("nav_drawer").resizable(false).exact_width(162.0)
            .frame(egui::Frame::none().fill(th.surface_container_low()).stroke(Stroke::new(1.0, th.outline_variant())))
            .show(ctx, |ui| {
                ui.add_space(14.0);
                ui.label(RichText::new("  类型模板").size(11.0).color(th.on_surface_variant()));
                ui.add_space(10.0);
                egui::ScrollArea::vertical().auto_shrink([false;2]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    for tmpl in all_templates() {
                        let active = self.project.template==tmpl.name;
                        // M3 Navigation Drawer item: 56dp height, RFULL pill, 12dp side margin
                        let avail_w = ui.available_width();
                        ui.horizontal(|ui| {
                            ui.add_space(8.0);
                            let (rect, resp) = ui.allocate_exact_size(
                                Vec2::new(avail_w - 16.0, 40.0),
                                egui::Sense::click(),
                            );
                            if ui.is_rect_visible(rect) {
                                // Layer 1: container fill
                                let fill = if active {
                                    th.secondary_container()
                                } else {
                                    Color32::TRANSPARENT
                                };
                                ui.painter().rect_filled(rect, RFULL, fill);

                                // Layer 2: state layer (M3 spec)
                                if resp.is_pointer_button_down_on() {
                                    let layer = if active { th.on_secondary_container() } else { th.on_surface() };
                                    ui.painter().rect_filled(rect, RFULL, ca(layer.r(), layer.g(), layer.b(), 30));
                                } else if resp.hovered() {
                                    let layer = if active { th.on_secondary_container() } else { th.on_surface() };
                                    ui.painter().rect_filled(rect, RFULL, ca(layer.r(), layer.g(), layer.b(), 20));
                                }

                                // Label
                                let label_col = if active {
                                    th.on_secondary_container()
                                } else if resp.hovered() {
                                    th.on_surface()
                                } else {
                                    th.on_surface_variant()
                                };
                                ui.painter().text(
                                    pos2(rect.min.x + 16.0, rect.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    tmpl.name,
                                    fnt(13.5),
                                    label_col,
                                );
                            }
                            if resp.hovered() { ctx.set_cursor_icon(egui::CursorIcon::PointingHand); }
                            if resp.clicked() { self.project.template = tmpl.name.to_string(); self.dirty=true; }
                        });
                    }
                });
            });
    }
}

// ═══════════════════════════════════════════════════════
//  右侧检查器
// ═══════════════════════════════════════════════════════
impl NovelApp {
    fn draw_inspector(&mut self, ctx: &egui::Context, th: Th, font_sz: f32) {
        egui::SidePanel::right("inspector").resizable(true).default_width(296.0).min_width(240.0).max_width(420.0)
            .frame(egui::Frame::none().fill(th.surface()).stroke(Stroke::new(1.0,th.outline_variant())).inner_margin(Margin::same(0.0)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false;2]).show(ui, |ui| {
                    let m = Margin::symmetric(16.0,0.0);
                    ui.add_space(18.0);

                    // ── 小说信息 ──────────────────────────────────
                    egui::Frame::none().inner_margin(m).show(ui, |ui| {
                        ui.label(RichText::new("📝  小说信息").size(13.0).color(th.on_surface()).strong());
                    });
                    ui.add_space(10.0);
                    egui::Frame::none().inner_margin(m).show(ui, |ui| {
                        card(ui, th, R12, Margin::same(16.0), |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.label(RichText::new("标题").size(11.5).color(th.on_surface_variant()));
                            ui.add_space(4.0);
                            let r = ui.add(egui::TextEdit::singleline(&mut self.title_buf)
                                .desired_width(f32::INFINITY).font(fnt(font_sz))
                                .text_color(th.on_surface()).hint_text("小说标题…").frame(false));
                            if r.changed() { self.project.title=self.title_buf.clone(); self.dirty=true; }

                            divider_h(ui, th);

                            // ── 章节数量 ──────────────────────────
                            ui.label(RichText::new("章节数量").size(11.5).color(th.on_surface_variant()));
                            ui.add_space(8.0);
                            const COUNT_PRESETS: &[(&str,usize)] = &[
                                ("10",10),("20",20),("30",30),
                                ("50",50),("80",80),("100",100),
                                ("150",150),("200",200),("300",300),
                            ];
                            let cur_c = self.project.target_chapters;
                            for row in COUNT_PRESETS.chunks(3) {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 6.0;
                                    for (lbl,n) in row {
                                        if chip(ui, lbl, cur_c==*n, th).clicked() {
                                            self.project.target_chapters=*n;
                                            self.count_buf=n.to_string();
                                            self.dirty=true;
                                        }
                                    }
                                });
                                ui.add_space(5.0);
                            }
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("自定义：").size(12.0).color(th.on_surface_variant()));
                                let r = ui.add(egui::TextEdit::singleline(&mut self.count_buf)
                                    .desired_width(54.0).text_color(th.on_surface()).hint_text("章节数").frame(false));
                                if r.changed() {
                                    if let Ok(n)=self.count_buf.parse::<usize>() { if n>0 { self.project.target_chapters=n.min(500); } }
                                    self.dirty=true;
                                }
                                ui.label(RichText::new("章").size(12.0).color(th.on_surface_variant()));
                            });
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("已选：{} 章",self.project.target_chapters)).size(12.0).color(th.primary()).strong());
                                if !self.project.chapters.is_empty() {
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(RichText::new(format!("已生成 {} 章",self.project.chapters.len())).size(11.0).color(th.on_surface_variant()));
                                    });
                                }
                            });

                            divider_h(ui, th);

                            // ── 每章字数 ──────────────────────────
                            ui.label(RichText::new("每章字数").size(11.5).color(th.on_surface_variant()));
                            ui.add_space(8.0);
                            const WORD_PRESETS: &[(&str,usize)] = &[
                                ("800",800),("1500",1500),("2000",2000),
                                ("3000",3000),("4000",4000),("6000",6000),
                            ];
                            let cur_w = self.project.target_words_per_chapter;
                            for row in WORD_PRESETS.chunks(3) {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 6.0;
                                    for (lbl,words) in row {
                                        if chip(ui, lbl, cur_w==*words, th).clicked() {
                                            self.project.target_words_per_chapter=*words;
                                            self.words_buf=words.to_string();
                                            self.dirty=true;
                                        }
                                    }
                                });
                                ui.add_space(5.0);
                            }
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("自定义：").size(12.0).color(th.on_surface_variant()));
                                let r = ui.add(egui::TextEdit::singleline(&mut self.words_buf)
                                    .desired_width(54.0).text_color(th.on_surface()).hint_text("字数").frame(false));
                                if r.changed() {
                                    if let Ok(n)=self.words_buf.parse::<usize>() { if n>=200 { self.project.target_words_per_chapter=n.min(10000); } }
                                    self.dirty=true;
                                }
                                ui.label(RichText::new("字").size(12.0).color(th.on_surface_variant()));
                            });
                        });
                    });

                    ui.add_space(20.0);

                    // ── 故事大纲 ──────────────────────────────────
                    egui::Frame::none().inner_margin(m).show(ui, |ui| {
                        ui.label(RichText::new("📋  故事大纲").size(13.0).color(th.on_surface()).strong());
                    });
                    ui.add_space(10.0);
                    egui::Frame::none().inner_margin(m).show(ui, |ui| {
                        card(ui, th, R12, Margin::same(16.0), |ui| {
                            ui.set_min_width(ui.available_width());
                            let r = ui.add(egui::TextEdit::multiline(&mut self.project.outline)
                                .desired_rows(6).desired_width(f32::INFINITY)
                                .font(fnt(font_sz-1.0)).text_color(th.on_surface())
                                .hint_text("故事背景、主角、主线剧情…").frame(false));
                            if r.changed() { self.dirty=true; }
                        });
                    });
                    ui.add_space(8.0);
                    egui::Frame::none().inner_margin(m).show(ui, |ui| {
                        let optg = self.gen_state==GenState::OptimizingOutline;
                        let lbl  = if optg {"⏳  优化中…"} else {"✨  AI 优化大纲"};
                        if btn_tonal(ui,lbl,!optg&&!self.gen_state.is_running(),th).clicked() {
                            if self.project.outline.trim().is_empty() { self.toast("请先填写大纲",ToastKind::Err); }
                            else {
                                self.gen_state=GenState::OptimizingOutline; self.worker.reset_stop();
                                self.worker.send(WorkerCmd::OptimizeOutline { outline:self.project.outline.clone(), template_name:self.project.template.clone(), extra_templates:self.project.extra_templates.clone(), custom_template_desc:self.project.custom_template_desc.clone() });
                                self.toast("AI 正在优化大纲…",ToastKind::Info);
                            }
                        }
                    });
                    if !self.project.optimized_outline.is_empty() {
                        ui.add_space(8.0);
                        egui::Frame::none().inner_margin(m).show(ui, |ui| {
                            egui::CollapsingHeader::new(RichText::new("✅  AI 优化版").size(12.5).color(th.on_tertiary_container()).strong())
                                .default_open(false).show(ui, |ui| {
                                    ui.add_space(4.0);
                                    card(ui, th, R12, Margin::same(12.0), |ui| {
                                        ui.set_min_width(ui.available_width());
                                        let r = ui.add(egui::TextEdit::multiline(&mut self.project.optimized_outline)
                                            .desired_rows(5).desired_width(f32::INFINITY)
                                            .font(fnt(12.5)).text_color(th.on_surface_variant()).frame(false));
                                        if r.changed() { self.dirty=true; }
                                    });
                                });
                        });
                    }

                    ui.add_space(20.0);

                    // ── 境界体系 ──────────────────────────────────
                    egui::Frame::none().inner_margin(m).show(ui, |ui| {
                        ui.label(RichText::new("🏆  境界体系").size(13.0).color(th.on_surface()).strong());
                    });
                    ui.add_space(10.0);
                    egui::Frame::none().inner_margin(m).show(ui, |ui| {
                        card(ui, th, R12, Margin::same(12.0), |ui| {
                            ui.set_min_width(ui.available_width());
                            self.draw_realm_picker(ui, th);
                        });
                    });

                    // ── 进度卡片 ──────────────────────────────────
                    if !self.project.chapters.is_empty() {
                        ui.add_space(20.0);
                        egui::Frame::none().inner_margin(m).show(ui, |ui| {
                            card_filled(ui, th, R12, Margin::same(16.0), |ui| {
                                ui.set_min_width(ui.available_width());
                                let done=self.project.completed_chapters(); let total=self.project.chapters.len();
                                let words=self.project.total_words(); let prog=done as f32/total as f32;
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(format!("{}/{} 章",done,total)).size(13.0).color(th.on_surface()).strong());
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(RichText::new(format!("{} 字",fmt_num(words))).size(12.0).color(th.on_surface_variant()));
                                    });
                                });
                                ui.add_space(10.0);
                                let avail=ui.available_width();
                                let (rect,_)=ui.allocate_exact_size(Vec2::new(avail,4.0), egui::Sense::hover());
                                ui.painter().rect_filled(rect, RFULL, th.on_surface_variant());
                                if prog>0.0 { let fr=Rect::from_min_size(rect.min,Vec2::new(rect.width()*prog,4.0)); ui.painter().rect_filled(fr,RFULL,self.gen_state.color(th)); }
                                ui.add_space(8.0);
                                ui.label(RichText::new(self.gen_state.label(total)).size(12.5).color(self.gen_state.color(th)));
                            });
                        });
                    }
                    ui.add_space(24.0);
                });
            });
    }

    fn draw_realm_picker(&mut self, ui: &mut egui::Ui, th: Th) {
        let realms = all_realms();
        let cats = [("修炼","⚡"),("武道","⚔️"),("玄幻","🐉"),("异能","🔮"),("末世","☠️"),("科幻","🚀"),("系统","💻"),("无限","🌀")];
        for (cat,_icon) in cats {
            let items: Vec<_> = realms.iter().filter(|r| r.category==cat).collect();
            if items.is_empty() { continue; }
            ui.horizontal(|ui| {
                ui.label(RichText::new(cat).size(11.0).color(th.on_surface_variant()).strong());
            });
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing=Vec2::new(4.0,4.0);
                for realm in &items {
                    let active = self.project.selected_realms.contains(&realm.id.to_string());
                    let resp = chip(ui, realm.name, active, th).on_hover_ui(|ui: &mut egui::Ui| {
                        ui.set_max_width(210.0);
                        ui.label(RichText::new(realm.name).size(13.0).color(th.primary()).strong());
                        ui.label(RichText::new(realm.levels).size(11.0).color(th.on_surface_variant()));
                        if !realm.description.is_empty() { ui.label(RichText::new(realm.description).size(10.5).color(th.outline())); }
                    });
                    if resp.clicked() {
                        let id=realm.id.to_string();
                        if active { self.project.selected_realms.retain(|x| x!=&id); }
                        else      { self.project.selected_realms.push(id); }
                        self.dirty=true;
                    }
                }
            });
            ui.add_space(8.0);
        }
        if !self.project.selected_realms.is_empty() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("已选 {} 个",self.project.selected_realms.len())).size(11.5).color(th.on_surface_variant()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if btn_text(ui,"清空",th.error()).clicked() { self.project.selected_realms.clear(); self.dirty=true; }
                });
            });
            ui.add_space(6.0);
        }
        ui.label(RichText::new("自定义境界（选填）").size(11.5).color(th.on_surface_variant()));
        ui.add_space(4.0);
        let r = ui.add(egui::TextEdit::multiline(&mut self.project.custom_realm)
            .desired_rows(2).desired_width(f32::INFINITY).font(fnt(12.5))
            .text_color(th.on_surface()).hint_text("如：力量→斗士→武将→战神…").frame(false));
        if r.changed() { self.dirty=true; }
    }
}

// ═══════════════════════════════════════════════════════
//  中央编辑区
// ═══════════════════════════════════════════════════════
impl NovelApp {
    fn draw_center(&mut self, ctx: &egui::Context, th: Th, font_sz: f32) {
        let center_fill = if !self.bg_frames.is_empty() { Color32::TRANSPARENT } else { th.surface() };
        // 向导模式：占据整个中央区域，不显示章节编辑器
        if self.show_wizard {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(center_fill).inner_margin(Margin::symmetric(40.0, 24.0)))
                .show(ctx, |ui| { self.draw_wizard_inline(ui, th); });
            if self.wizard_realm_dialog.is_some() { self.draw_wizard_realm_dialog(ctx, th); }
            return;
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(center_fill).inner_margin(Margin::same(0.0)))
            .show(ctx, |ui| {
                // 章节 Tab 栏
                let tabs_fill = th.surface_container_low();
                egui::TopBottomPanel::top("ch_tabs").exact_height(44.0)
                    .frame(egui::Frame::none().fill(tabs_fill).stroke(Stroke::new(1.0,th.outline_variant())).inner_margin(Margin::symmetric(12.0,6.0)))
                    .show_inside(ui, |ui| {
                        if self.project.chapters.is_empty() {
                            ui.vertical_centered(|ui| {
                                ui.add_space(6.0);
                                ui.label(RichText::new("填写大纲后点击「▶ 开始生成」").size(12.5).color(th.on_surface_variant()));
                            });
                        } else {
                            egui::ScrollArea::horizontal().id_salt("ch_sc").show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x=3.0;
                                    let len=self.project.chapters.len();
                                    for i in 0..len {
                                        let active = self.selected_chapter==Some(i);
                                        let (bg,fg) = match &self.project.chapters[i].status {
                                            ChapterStatus::Done => if active { (th.secondary_container(), th.on_secondary_container()) }
                                                else { (blend(th.surface_container(), th.on_tertiary_container(), 50), th.on_tertiary_container()) },
                                            ChapterStatus::Generating => (blend(th.surface_container(), c(200,120,0), 50), c(200,120,0)),
                                            ChapterStatus::Error(_)   => (th.error_container(), th.on_error_container()),
                                            ChapterStatus::Pending    => if active { (th.surface_container_highest(), th.on_surface()) }
                                                else { (Color32::TRANSPARENT, th.on_surface_variant()) },
                                        };
                                        let (rect,resp)=ui.allocate_exact_size(Vec2::new(32.0,30.0), egui::Sense::click());
                                        if ui.is_rect_visible(rect) {
                                            ui.painter().rect_filled(rect, R8, bg);
                                            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER,
                                                format!("{}",self.project.chapters[i].number), fnt(12.5), fg);
                                        }
                                        if resp.clicked() { self.selected_chapter=Some(i); }
                                    }
                                });
                            });
                        }
                    });

                // 章节标题行
                if let Some(idx) = self.safe_idx() {
                    let ch = &self.project.chapters[idx];
                    let (cn,ct,cb,cw,ce) = (ch.number,ch.title.clone(),ch.brief.clone(),ch.word_count,matches!(ch.status,ChapterStatus::Error(_)));
                    egui::TopBottomPanel::top("ch_title_bar").exact_height(36.0)
                        .frame(egui::Frame::none().fill(center_fill).stroke(Stroke::new(1.0,th.outline_variant())).inner_margin(Margin::symmetric(22.0,6.0)))
                        .show_inside(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("第 {} 章",cn)).size(12.0).color(th.on_surface_variant()));
                                ui.add_space(6.0);
                                ui.label(RichText::new(&ct).size(13.0).color(th.on_surface()).strong());
                                if !cb.is_empty() { ui.label(RichText::new(format!("· {}",cb)).size(12.0).color(th.on_surface_variant())); }
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(RichText::new(format!("{} 字",fmt_num(cw))).size(12.0).color(th.on_surface_variant()));
                                    if ce { ui.add_space(8.0); if btn_tonal(ui,"↺ 重试",true,th).clicked() { self.launch(idx); } }
                                });
                            });
                        });
                }

                // 主编辑区
                egui::CentralPanel::default()
                    .frame(egui::Frame::none().fill(center_fill).inner_margin(Margin::symmetric(32.0,20.0)))
                    .show_inside(ui, |ui| {
                        let idx = match self.safe_idx() { Some(i)=>i, None=>{ self.draw_welcome(ui,th); return; } };
                        let is_gen = self.project.chapters[idx].status==ChapterStatus::Generating;
                        if let ChapterStatus::Error(e) = self.project.chapters[idx].status.clone() {
                            egui::Frame::none().fill(th.error_container()).rounding(R12).inner_margin(Margin::symmetric(16.0,10.0)).show(ui, |ui| {
                                ui.label(RichText::new(format!("⚠  {}",e)).size(13.0).color(th.on_error_container()));
                            });
                            ui.add_space(12.0);
                        }
                        let mut content_changed = false;
                        egui::ScrollArea::vertical().id_salt(format!("ed_{}",idx)).stick_to_bottom(is_gen).auto_shrink([false;2]).show(ui, |ui| {
                            let ch = &mut self.project.chapters[idx];
                            let r = ui.add(egui::TextEdit::multiline(&mut ch.content)
                                .desired_width(f32::INFINITY).desired_rows(28)
                                .font(fnt(font_sz)).text_color(th.on_surface())
                                .frame(false).hint_text("章节内容将在此实时显示…"));
                            if r.changed() { ch.update_word_count(); content_changed = true; }
                        });
                        if content_changed { self.dirty = true; }
                        if is_gen {
                            ui.add_space(10.0);
                            card(ui, th, R12, Margin::symmetric(14.0,10.0), |ui| {
                                ui.horizontal(|ui| {
                                    ui.add(egui::Spinner::new().size(14.0).color(th.primary()));
                                    ui.add_space(8.0);
                                    ui.label(RichText::new("AI 创作中…").size(13.0).color(th.primary()));
                                });
                            });
                        }
                    });
            });
    }

    fn draw_welcome(&mut self, ui: &mut egui::Ui, th: Th) {
        let blurred = self.bg_blurred.clone();
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            let avail = ui.available_width().min(520.0);
            ui.allocate_ui_with_layout(Vec2::new(avail, 0.0), egui::Layout::top_down(egui::Align::Center), |ui| {
                card_or_frosted(ui, th, R28, Margin::symmetric(28.0, 32.0), blurred.as_ref(), |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("✍").size(72.0));
                        ui.add_space(16.0);
                        ui.label(RichText::new("AI 全自动小说创作").size(26.0).color(th.on_surface()).strong());
                        ui.add_space(8.0);
                        ui.label(RichText::new("4 步向导 → 一键生成全书").size(13.5).color(th.on_surface_variant()));
                        ui.add_space(32.0);
                        if btn_filled(ui, "✨  新建小说", true, th).clicked() {
                            self.wizard_proj = NovelProject::default();
                            self.wizard_count_buf = self.wizard_proj.target_chapters.to_string();
                            self.wizard_words_buf = self.wizard_proj.target_words_per_chapter.to_string();
                            self.wizard_page = 0;
                            self.wizard_realm_dialog = None;
                            self.show_wizard = true;
                        }
                        ui.add_space(10.0);
                        if btn_outlined(ui, "📂  打开已有项目", true, th).clicked() { self.do_open(); }
                    });
                });
            });
        });
    }
}

// ═══════════════════════════════════════════════════════
//  Toast
// ═══════════════════════════════════════════════════════
impl NovelApp {
    fn draw_toasts(&self, ctx: &egui::Context, th: Th) {
        if self.toasts.is_empty() { return; }
        let screen  = ctx.screen_rect();
        let mut y   = screen.max.y - 32.0;
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("toasts")));
        for t in self.toasts.iter().rev() {
            let a   = (t.alpha()*255.0) as u8;
            let gal = ctx.fonts(|f| f.layout_no_wrap(t.msg.clone(), fnt(13.5), Color32::WHITE));
            let sz  = gal.size();
            let rect = Rect::from_center_size(pos2(screen.center().x, y-sz.y/2.0-10.0), Vec2::new(sz.x+40.0,sz.y+20.0));
            let bg=t.bg(th); let fg=t.fg(th); let a16=a as u16;
            painter.rect_filled(rect, R12, ca(bg.r(),bg.g(),bg.b(),(a16*220/255) as u8));
            painter.rect_stroke(rect, R12, Stroke::new(1.0, ca(fg.r(),fg.g(),fg.b(),(a16*60/255) as u8)));
            painter.galley(rect.min+Vec2::new(20.0,10.0), gal, ca(fg.r(),fg.g(),fg.b(),a));
            y -= rect.height()+8.0;
        }
        ctx.request_repaint();
    }
}

// ═══════════════════════════════════════════════════════
//  设置窗口
// ═══════════════════════════════════════════════════════
impl NovelApp {
    fn draw_settings(&mut self, ctx: &egui::Context, th: Th) {
        let win_fill = th.surface_container_low();
        let mut open = self.show_settings;
        egui::Window::new("⚙  设置").open(&mut open).resizable(false).collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).default_width(500.0)
            .frame(egui::Frame::window(&ctx.style()).fill(win_fill).stroke(Stroke::new(1.0,th.outline_variant())).rounding(R28))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().max_height(580.0).show(ui, |ui| {
                    // ── API ───────────────────────────────────────
                    ui.label(RichText::new("🔌  API 接口").size(14.0).color(th.on_surface()).strong());
                    ui.add_space(8.0);
                    ui.label(RichText::new("提供商").size(11.5).color(th.on_surface_variant()));
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing=Vec2::new(6.0,6.0);
                        for p in provider_presets() {
                            let active=self.s_provider==p.name;
                            if chip(ui, p.name, active, th).clicked() {
                                self.s_provider=p.name.to_string();
                                if p.name!="自定义" { self.s_base_url=p.base_url.to_string(); if !p.models.is_empty() { self.s_model=p.models[0].to_string(); self.s_model_input=self.s_model.clone(); } }
                            }
                        }
                    });
                    ui.add_space(8.0);
                    ui.label(RichText::new("Base URL").size(11.5).color(th.on_surface_variant()));
                    ui.add(egui::TextEdit::singleline(&mut self.s_base_url).desired_width(f32::INFINITY).text_color(th.on_surface()).hint_text("https://api.example.com/v1"));
                    ui.add_space(6.0);
                    ui.label(RichText::new("API Key").size(11.5).color(th.on_surface_variant()));
                    ui.add(egui::TextEdit::singleline(&mut self.s_api_key).desired_width(f32::INFINITY).password(true).text_color(th.on_surface()).hint_text("sk-…"));
                    ui.add_space(6.0);
                    ui.label(RichText::new("模型").size(11.5).color(th.on_surface_variant()));
                    let presets = provider_presets();
                    if let Some(p) = presets.iter().find(|p| p.name==self.s_provider) {
                        if !p.models.is_empty() {
                            ui.add_space(4.0);
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing=Vec2::new(6.0,6.0);
                                for m in p.models { let active=self.s_model==*m; if chip(ui,m,active,th).clicked() { self.s_model=m.to_string(); self.s_model_input=m.to_string(); } }
                            });
                            ui.add_space(4.0);
                        }
                    }
                    if ui.add(egui::TextEdit::singleline(&mut self.s_model_input).desired_width(f32::INFINITY).text_color(th.on_surface()).hint_text("手动输入模型名…")).changed() { self.s_model=self.s_model_input.clone(); }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| { ui.label(RichText::new("Temperature").size(13.0).color(th.on_surface())); ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { ui.add(egui::Slider::new(&mut self.s_temperature,0.0..=2.0).step_by(0.05)); }); });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| { ui.label(RichText::new("Max Tokens").size(13.0).color(th.on_surface())); ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { ui.add(egui::TextEdit::singleline(&mut self.s_max_tokens).desired_width(80.0).text_color(th.on_surface())); }); });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| { ui.label(RichText::new("编辑器字号").size(13.0).color(th.on_surface())); ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { ui.add(egui::Slider::new(&mut self.s_font_size,11.0..=24.0).step_by(1.0)); }); });
                    if let Some((ok,msg)) = &self.s_test_result {
                        ui.add_space(6.0);
                        let (bg,fg) = if *ok {(th.tertiary_container(),th.on_tertiary_container())} else {(th.error_container(),th.on_error_container())};
                        egui::Frame::none().fill(bg).rounding(R12).inner_margin(Margin::same(12.0)).show(ui, |ui| { ui.label(RichText::new(msg).size(13.0).color(fg)); });
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if btn_text(ui,"🔌  测试连接",th.primary()).clicked() {
                            let cfg=AppConfig{provider:self.s_provider.clone(),api_key:self.s_api_key.clone(),base_url:self.s_base_url.clone(),model:self.s_model.clone(),temperature:self.s_temperature,max_tokens:self.s_max_tokens.parse().unwrap_or(4096),font_size:self.s_font_size,auto_save:true,setup_done:true};
                            match crate::api::test_connection(&cfg) {
                                Ok(r)  => self.s_test_result=Some((true,  format!("✓ 连接成功  {}",&r[..r.len().min(60)]))),
                                Err(e) => self.s_test_result=Some((false, format!("✗ {}",e))),
                            }
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if btn_filled(ui,"保存设置",true,th).clicked() {
                                let cfg=AppConfig{provider:self.s_provider.clone(),api_key:self.s_api_key.clone(),base_url:self.s_base_url.clone(),model:self.s_model.clone(),temperature:self.s_temperature,max_tokens:self.s_max_tokens.parse().unwrap_or(4096),font_size:self.s_font_size,auto_save:true,setup_done:true};
                                cfg.save(); *self.config.lock().unwrap()=cfg; self.show_settings=false; self.toast("设置已保存 ✓",ToastKind::Ok);
                            }
                        });
                    });

                    ui.add_space(20.0);
                    ui.add(egui::Separator::default().spacing(4.0));
                    ui.add_space(12.0);

                    // ── 外观 ──────────────────────────────────────
                    ui.label(RichText::new("🎨  外观").size(14.0).color(th.on_surface()).strong());
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("配色模式").size(13.0).color(th.on_surface()));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if btn_tonal(ui, if self.dark_mode {"🌙 深色"} else {"☀ 浅色"}, true, th).clicked() { self.dark_mode=!self.dark_mode; }
                        });
                    });

                    ui.add_space(16.0);
                    ui.add(egui::Separator::default().spacing(4.0));
                    ui.add_space(12.0);

                    // ── 自定义背景 ────────────────────────────────
                    ui.label(RichText::new("🖼  自定义背景").size(14.0).color(th.on_surface()).strong());
                    ui.add_space(6.0);

                    ui.add_space(12.0);

                    // 背景图来源
                    let cur_src = match &self.bg_source {
                        BgSource::None     => "当前：无背景".into(),
                        BgSource::Local(p) => format!("📁  {}", p.file_name().and_then(|n|n.to_str()).unwrap_or("…")),
                        BgSource::Url(u)   => format!("🔗  {}", if u.len()>45 {&u[..45]} else {u}),
                    };
                    ui.label(RichText::new(cur_src).size(12.0).color(th.on_surface_variant()));
                    ui.add_space(8.0);
                    ui.label(RichText::new("图片链接（JPG / PNG / WebP / GIF）").size(11.5).color(th.on_surface_variant()));
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.bg_url_input).desired_width(ui.available_width()-116.0).text_color(th.on_surface()).hint_text("https://…"));
                        let loading=self.bg_loading;
                        if btn_tonal(ui, if loading {"加载中…"} else {"从链接加载"}, !loading, th).clicked() { self.load_bg_url(); }
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if btn_tonal(ui,"📁  本地图片",true,th).clicked() { self.pick_local_bg=true; }
                        ui.add_space(6.0);
                        if btn_outlined(ui,"✕  清除背景",!self.bg_frames.is_empty(),th).clicked() {
                            self.clear_bg();
                            self.toast("背景已清除",ToastKind::Info);
                        }
                    });
                });
            });
        self.show_settings = open;
    }
}

// ═══════════════════════════════════════════════════════
//  关于窗口
// ═══════════════════════════════════════════════════════
impl NovelApp {
    fn draw_about(&mut self, ctx: &egui::Context, th: Th) {
        let win_fill = th.surface_container_low();
        let mut open = self.show_about;
        egui::Window::new("关于").open(&mut open).resizable(false).collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).default_width(340.0)
            .frame(egui::Frame::window(&ctx.style()).fill(win_fill).stroke(Stroke::new(1.0,th.outline_variant())).rounding(R28))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(12.0);
                    ui.label(RichText::new("✍").size(56.0));
                    ui.add_space(8.0);
                    ui.label(RichText::new("AI 小说创作工具").size(20.0).color(th.on_surface()).strong());
                    ui.add_space(4.0);
                    ui.label(RichText::new("v1.0  ·  Rust + egui  ·  Material Design 3").size(12.0).color(th.on_surface_variant()));
                    ui.add_space(16.0);
                    card(ui, th, R16, Margin::same(16.0), |ui| {
                        ui.set_max_width(288.0);
                        for line in ["DeepSeek · ChatGPT · Gemini · Ollama","12 种模板  ·  22 种境界体系  ·  全自动续写","章节数量自由选择（10 ～ 300 章）","自定义背景图  ·  亮色 / 暗色","⌘S / Ctrl+S 快速保存"] {
                            ui.label(RichText::new(line).size(12.5).color(th.on_surface_variant())); ui.add_space(4.0);
                        }
                    });
                    ui.add_space(12.0);
                    card(ui, th, R16, Margin::symmetric(20.0,12.0), |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("🐙").size(20.0));
                            ui.add_space(8.0);
                            ui.vertical(|ui| {
                                ui.label(RichText::new("GitHub").size(11.0).color(th.on_surface_variant()));
                                let link = ui.add(egui::Label::new(RichText::new("github.com/Ethan13322836698").size(13.5).color(th.primary())).sense(egui::Sense::click()));
                                if link.hovered() { ctx.set_cursor_icon(egui::CursorIcon::PointingHand); }
                                if link.clicked() { ctx.open_url(egui::OpenUrl::new_tab("https://github.com/Ethan13322836698")); }
                            });
                        });
                    });
                    ui.add_space(12.0);
                });
            });
        self.show_about = open;
    }
}

// ═══════════════════════════════════════════════════════
//  Save Prompt & Wizard
// ═══════════════════════════════════════════════════════
impl NovelApp {
    fn draw_save_prompt(&mut self, ctx: &egui::Context, th: Th) {
        let win_fill = th.surface_container_low();
        egui::Window::new("未保存的更改").resizable(false).collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).default_width(360.0)
            .frame(egui::Frame::window(&ctx.style()).fill(win_fill).stroke(Stroke::new(1.0,th.outline_variant())).rounding(R28))
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.label(RichText::new("项目有未保存的修改，是否保存？").size(13.5).color(th.on_surface()));
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    if btn_filled(ui,"保存",true,th).clicked() {
                        self.do_save();
                        if !self.dirty {
                            self.show_save_prompt = false;
                            self.perform_save_prompt_action(ctx);
                        }
                    }
                    ui.add_space(6.0);
                    if btn_tonal(ui,"不保存",true,th).clicked() {
                        self.dirty = false;
                        self.show_save_prompt = false;
                        self.perform_save_prompt_action(ctx);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if btn_outlined(ui,"取消",true,th).clicked() {
                            self.show_save_prompt = false;
                        }
                    });
                });
                ui.add_space(4.0);
            });
    }

    fn draw_wizard_inline(&mut self, ui: &mut egui::Ui, th: Th) {
        let page = self.wizard_page;
        let mut cancel = false;
        let mut next = false;
        let mut prev = false;
        let mut finish = false;

        // 居中：用左右留白 + 居中布局，确保内容获得正确宽度
        let avail_w = ui.available_width();
        let max_w = avail_w.min(720.0);
        let pad_x = ((avail_w - max_w) / 2.0).max(0.0);

        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            // 顶部留点空间
            ui.add_space(4.0);
        });

        // 居中容器：在外层 horizontal 中给定明确宽度的子区域
        let total_h = ui.available_height();
        let (rect, _) = ui.allocate_exact_size(Vec2::new(avail_w, total_h), egui::Sense::hover());
        let inner_rect = Rect::from_min_size(
            pos2(rect.min.x + pad_x, rect.min.y),
            Vec2::new(max_w, total_h),
        );
        // 毛玻璃背板：若加载了背景，给整个向导区域绘制半透模糊底
        if let Some(blur) = self.bg_blurred.clone() {
            let screen = ui.ctx().screen_rect();
            let sw = screen.width().max(1.0); let sh = screen.height().max(1.0);
            let uv = Rect::from_min_max(
                pos2((inner_rect.min.x - screen.min.x)/sw, (inner_rect.min.y - screen.min.y)/sh),
                pos2((inner_rect.max.x - screen.min.x)/sw, (inner_rect.max.y - screen.min.y)/sh),
            );
            let tint = if th.dark { ca(8,10,20,150) } else { ca(255,255,255,160) };
            let p = ui.painter();
            p.image(blur.id(), inner_rect, uv, Color32::WHITE);
            p.rect_filled(inner_rect, R28, tint);
            p.rect_stroke(inner_rect, R28, Stroke::new(1.0, ca(255,255,255,60)));
        }
        let mut content_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(inner_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        let ui = &mut content_ui;

        // Header
        ui.horizontal(|ui| {
            ui.label(RichText::new("✨  新建小说").size(22.0).color(th.on_surface()).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if btn_text(ui, "✕ 取消", th.on_surface_variant()).clicked() { cancel = true; }
            });
        });
        ui.add_space(10.0);

        // Progress
        ui.horizontal(|ui| {
            let titles = ["类型模板","章节字数","修炼体系","标题大纲"];
            for (i, t) in titles.iter().enumerate() {
                let active = i as u8 == page;
                let done = (i as u8) < page;
                let col = if active { th.primary() } else if done { th.on_tertiary_container() } else { th.on_surface_variant() };
                let sym = if done { "✓" } else { "○" };
                ui.label(RichText::new(format!("{}  {}.{}", sym, i+1, t)).size(12.5).color(col).strong());
                if i < titles.len()-1 { ui.add_space(4.0); ui.label(RichText::new("›").size(12.0).color(th.on_surface_variant())); ui.add_space(4.0); }
            }
        });
        ui.add_space(8.0);
        ui.add(egui::Separator::default().spacing(6.0));
        ui.add_space(12.0);

        let avail_h = ui.available_height() - 70.0;
        egui::ScrollArea::vertical().id_salt("wizard_scroll").max_height(avail_h.max(200.0)).auto_shrink([false;2]).show(ui, |ui| {
            ui.set_min_width(max_w - 8.0);
            match page {
                0 => self.wizard_page_template(ui, th),
                1 => self.wizard_page_counts(ui, th),
                2 => self.wizard_page_realms(ui, th),
                _ => self.wizard_page_outline(ui, th),
            }
        });

        ui.add_space(12.0);
        ui.add(egui::Separator::default().spacing(4.0));
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if page > 0 { if btn_outlined(ui,"← 上一步",true,th).clicked() { prev = true; } }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if page < 3 {
                    if btn_filled(ui,"下一步 →",true,th).clicked() { next = true; }
                } else {
                    if btn_filled(ui,"✓  完成",true,th).clicked() { finish = true; }
                }
            });
        });

        if cancel { self.show_wizard = false; self.wizard_realm_dialog = None; }
        if prev && self.wizard_page > 0 { self.wizard_page -= 1; }
        if next && self.wizard_page < 3 { self.wizard_page += 1; }
        if finish { self.finish_wizard(); }
    }

    fn wizard_page_template(&mut self, ui: &mut egui::Ui, th: Th) {
        ui.label(RichText::new("选择小说类型（可多选，将合并多种风格）").size(13.0).color(th.on_surface_variant()));
        ui.add_space(10.0);
        let tmpls = all_templates();
        let names: Vec<&str> = tmpls.iter().map(|t| t.name).collect();
        let mut all_names: Vec<&str> = names.clone();
        all_names.push("自定义");
        // 主模板是否是预设
        let is_preset = names.contains(&self.wizard_proj.template.as_str())
            || (self.wizard_proj.template.is_empty() && !self.wizard_proj.extra_templates.is_empty());
        let is_selected = |proj: &NovelProject, n: &str| -> bool {
            proj.template == n || proj.extra_templates.iter().any(|x| x == n)
        };
        for row in all_names.chunks(3) {
            ui.horizontal(|ui| {
                for name in row {
                    let active = if *name == "自定义" {
                        !names.contains(&self.wizard_proj.template.as_str()) && !self.wizard_proj.template.is_empty()
                    } else {
                        is_selected(&self.wizard_proj, name)
                    };
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(170.0, 56.0), egui::Sense::click());
                    if ui.is_rect_visible(rect) {
                        let bg = if active { th.secondary_container() }
                            else if resp.hovered() { th.hover_state(th.surface_container()) }
                            else { th.surface_container() };
                        let fg = if active { th.on_secondary_container() } else { th.on_surface() };
                        ui.painter().rect_filled(rect, R12, bg);
                        ui.painter().rect_stroke(rect, R12, Stroke::new(1.0, th.outline_variant()));
                        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, *name, fnt(14.0), fg);
                    }
                    if resp.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
                    if resp.clicked() {
                        if *name == "自定义" {
                            // 切换到自定义：清掉所有预设主模板与额外预设，让用户输入自定义名
                            if names.contains(&self.wizard_proj.template.as_str()) {
                                self.wizard_proj.template.clear();
                            }
                            self.wizard_proj.extra_templates.retain(|n| !names.contains(&n.as_str()));
                        } else {
                            let n = name.to_string();
                            if is_selected(&self.wizard_proj, name) {
                                // 取消选择
                                if self.wizard_proj.template == n {
                                    // 主模板被取消：把第一个 extra 提升为主
                                    if self.wizard_proj.extra_templates.is_empty() {
                                        self.wizard_proj.template.clear();
                                    } else {
                                        self.wizard_proj.template = self.wizard_proj.extra_templates.remove(0);
                                    }
                                } else {
                                    self.wizard_proj.extra_templates.retain(|x| x != &n);
                                }
                            } else {
                                // 新增
                                if self.wizard_proj.template.is_empty() {
                                    self.wizard_proj.template = n;
                                } else {
                                    self.wizard_proj.extra_templates.push(n);
                                }
                            }
                        }
                    }
                }
            });
            ui.add_space(6.0);
        }
        // 已选数量提示
        {
            let mut total = 0usize;
            if !self.wizard_proj.template.is_empty() { total += 1; }
            total += self.wizard_proj.extra_templates.len();
            ui.add_space(4.0);
            ui.label(RichText::new(format!("已选 {} 个", total)).size(11.5).color(th.on_surface_variant()));
        }
        if !is_preset {
            ui.add_space(12.0);
            card(ui, th, R12, Margin::same(14.0), |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(RichText::new("自定义类型名称").size(11.5).color(th.on_surface_variant()));
                ui.add_space(4.0);
                ui.add(egui::TextEdit::singleline(&mut self.wizard_proj.template)
                    .desired_width(f32::INFINITY).text_color(th.on_surface())
                    .hint_text("如：仙侠现代、克苏鲁、星际机甲…").frame(false));

                ui.add_space(12.0);
                ui.label(RichText::new("题材简介 / 创作要求").size(11.5).color(th.on_surface_variant()));
                ui.add_space(4.0);
                ui.add(egui::TextEdit::multiline(&mut self.wizard_proj.custom_template_desc)
                    .desired_rows(5).desired_width(f32::INFINITY)
                    .font(fnt(13.0)).text_color(th.on_surface())
                    .hint_text("告诉 AI 这是什么样的小说、文风偏好、世界观设定。例：\n\n克苏鲁风格悬疑短篇，主角是民国侦探，文风冷峻克制；\n场景多在阴雨海港，重视氛围铺垫，避免直白展示怪物。")
                    .frame(false));
                ui.add_space(4.0);
                ui.label(RichText::new("提示：填得越具体，AI 越贴合你想要的风格").size(11.0).color(th.outline()));
            });
        }
    }

    fn wizard_page_counts(&mut self, ui: &mut egui::Ui, th: Th) {
        ui.label(RichText::new("章节数量").size(13.0).color(th.on_surface_variant()).strong());
        ui.add_space(8.0);
        const COUNT_PRESETS: &[(&str,usize)] = &[
            ("10",10),("20",20),("30",30),("50",50),("80",80),
            ("100",100),("150",150),("200",200),("300",300),
        ];
        let cur_c = self.wizard_proj.target_chapters;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0,6.0);
            for (lbl,n) in COUNT_PRESETS {
                if chip(ui, lbl, cur_c==*n, th).clicked() {
                    self.wizard_proj.target_chapters = *n;
                    self.wizard_count_buf = n.to_string();
                }
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("自定义：").size(12.0).color(th.on_surface_variant()));
            let r = ui.add(egui::TextEdit::singleline(&mut self.wizard_count_buf)
                .desired_width(70.0).text_color(th.on_surface()).hint_text("章数"));
            if r.changed() {
                if let Ok(n)=self.wizard_count_buf.parse::<usize>() { if n>0 { self.wizard_proj.target_chapters=n.min(500); } }
            }
            ui.label(RichText::new("章").size(12.0).color(th.on_surface_variant()));
        });

        ui.add_space(16.0);
        ui.label(RichText::new("每章字数").size(13.0).color(th.on_surface_variant()).strong());
        ui.add_space(8.0);
        const WORD_PRESETS: &[(&str,usize)] = &[
            ("800",800),("1500",1500),("2000",2000),("3000",3000),("4000",4000),("6000",6000),
        ];
        let cur_w = self.wizard_proj.target_words_per_chapter;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0,6.0);
            for (lbl,n) in WORD_PRESETS {
                if chip(ui, lbl, cur_w==*n, th).clicked() {
                    self.wizard_proj.target_words_per_chapter = *n;
                    self.wizard_words_buf = n.to_string();
                }
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("自定义：").size(12.0).color(th.on_surface_variant()));
            let r = ui.add(egui::TextEdit::singleline(&mut self.wizard_words_buf)
                .desired_width(70.0).text_color(th.on_surface()).hint_text("字数"));
            if r.changed() {
                if let Ok(n)=self.wizard_words_buf.parse::<usize>() { if n>=200 { self.wizard_proj.target_words_per_chapter=n.min(10000); } }
            }
            ui.label(RichText::new("字").size(12.0).color(th.on_surface_variant()));
        });
    }

    fn wizard_page_realms(&mut self, ui: &mut egui::Ui, th: Th) {
        ui.label(RichText::new("预设境界（可多选）").size(13.0).color(th.on_surface_variant()).strong());
        ui.add_space(8.0);
        let realms = all_realms();
        let cats = ["修炼","武道","玄幻","异能","末世","科幻","系统","无限"];
        for cat in cats {
            let items: Vec<_> = realms.iter().filter(|r| r.category==cat).collect();
            if items.is_empty() { continue; }
            ui.label(RichText::new(cat).size(11.0).color(th.on_surface_variant()).strong());
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(4.0,4.0);
                for realm in &items {
                    let active = self.wizard_proj.selected_realms.contains(&realm.id.to_string());
                    if chip(ui, realm.name, active, th).clicked() {
                        let id = realm.id.to_string();
                        if active { self.wizard_proj.selected_realms.retain(|x| x!=&id); }
                        else { self.wizard_proj.selected_realms.push(id); }
                    }
                }
            });
            ui.add_space(6.0);
        }

        ui.add_space(10.0);
        ui.add(egui::Separator::default().spacing(4.0));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("自定义境界").size(13.0).color(th.on_surface_variant()).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if btn_tonal(ui,"+ 新境界",true,th).clicked() {
                    let next_order = self.wizard_proj.custom_realms.iter().map(|r| r.order).max().unwrap_or(0) + 1;
                    self.wizard_realm_dialog = Some(CustomRealm {
                        order: next_order, name: String::new(),
                        description: String::new(), sub_levels: String::new(),
                    });
                }
            });
        });
        ui.add_space(6.0);
        let mut to_remove: Option<usize> = None;
        let mut list = self.wizard_proj.custom_realms.clone();
        list.sort_by_key(|r| r.order);
        for (i, r) in list.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("#{}", r.order)).size(12.0).color(th.primary()).strong());
                ui.add_space(6.0);
                ui.label(RichText::new(&r.name).size(13.0).color(th.on_surface()).strong());
                if !r.sub_levels.is_empty() {
                    ui.label(RichText::new(format!("· {}", r.sub_levels)).size(11.5).color(th.on_surface_variant()));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if btn_text(ui,"删除",th.error()).clicked() { to_remove = Some(i); }
                });
            });
            ui.add_space(4.0);
        }
        if let Some(i) = to_remove {
            if let Some(target) = list.get(i) {
                let order = target.order;
                let name = target.name.clone();
                self.wizard_proj.custom_realms.retain(|r| !(r.order == order && r.name == name));
            }
        }
    }

    fn wizard_page_outline(&mut self, ui: &mut egui::Ui, th: Th) {
        ui.label(RichText::new("小说标题").size(11.5).color(th.on_surface_variant()));
        ui.add_space(4.0);
        ui.add(egui::TextEdit::singleline(&mut self.wizard_proj.title)
            .desired_width(f32::INFINITY).text_color(th.on_surface()).hint_text("小说标题…"));
        ui.add_space(12.0);

        ui.label(RichText::new("故事大纲").size(11.5).color(th.on_surface_variant()));
        ui.add_space(4.0);
        ui.add(egui::TextEdit::multiline(&mut self.wizard_proj.outline)
            .desired_rows(6).desired_width(f32::INFINITY)
            .text_color(th.on_surface()).hint_text("故事背景、主角、主线剧情…"));
        ui.add_space(12.0);

        ui.checkbox(&mut self.wizard_proj.reduce_ai_traits, "降低 AI 写作味");
        ui.add_space(4.0);
        ui.checkbox(&mut self.wizard_proj.avoid_famous_names, "防止主角名与知名小说雷同");
        ui.add_space(12.0);

        ui.horizontal(|ui| {
            ui.label(RichText::new("模型设置：").size(12.0).color(th.on_surface_variant()));
            if btn_text(ui,"前往设置",th.primary()).clicked() {
                self.show_wizard = false;
                self.show_settings = true;
            }
        });
    }

    fn draw_wizard_realm_dialog(&mut self, ctx: &egui::Context, th: Th) {
        let win_fill = th.surface_container_low();
        let mut close = false;
        let mut save = false;
        if let Some(realm) = self.wizard_realm_dialog.as_mut() {
            let header_text = if realm.name.is_empty() { "新境界" } else { "编辑境界" };
            // 用固定 id 防止标题变化导致 Window 重建丢焦点
            egui::Window::new("realm_dialog")
                .id(egui::Id::new("realm_dialog_fixed"))
                .title_bar(false)
                .resizable(false).collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::new(0.0, 0.0)).default_width(420.0)
                .frame(egui::Frame::window(&ctx.style()).fill(win_fill).stroke(Stroke::new(1.0,th.outline_variant())).rounding(R28).inner_margin(Margin::same(20.0)))
                .show(ctx, |ui| {
                    ui.label(RichText::new(header_text).size(16.0).color(th.on_surface()).strong());
                    ui.add_space(12.0);

                    ui.label(RichText::new("第几个境界（数字越大越强）").size(11.5).color(th.on_surface_variant()));
                    ui.add_space(4.0);
                    ui.add(egui::DragValue::new(&mut realm.order).range(1..=999));
                    ui.add_space(10.0);

                    ui.label(RichText::new("名字").size(11.5).color(th.on_surface_variant()));
                    ui.add_space(4.0);
                    ui.add(egui::TextEdit::singleline(&mut realm.name)
                        .id(egui::Id::new("realm_name_edit"))
                        .desired_width(f32::INFINITY).hint_text("如：练气期"));
                    ui.add_space(10.0);

                    ui.label(RichText::new("描述").size(11.5).color(th.on_surface_variant()));
                    ui.add_space(4.0);
                    ui.add(egui::TextEdit::multiline(&mut realm.description)
                        .id(egui::Id::new("realm_desc_edit"))
                        .desired_rows(3).desired_width(f32::INFINITY).hint_text("此境界的特点…"));
                    ui.add_space(10.0);

                    ui.label(RichText::new("小境界划分").size(11.5).color(th.on_surface_variant()));
                    ui.add_space(4.0);
                    ui.add(egui::TextEdit::singleline(&mut realm.sub_levels)
                        .id(egui::Id::new("realm_sub_edit"))
                        .desired_width(f32::INFINITY).hint_text("用 / 或 ／ 或 → 分隔，如 初期 / 中期 / 大圆满"));
                    ui.add_space(14.0);

                    ui.horizontal(|ui| {
                        if btn_filled(ui,"保存",true,th).clicked() { save = true; }
                        ui.add_space(6.0);
                        if btn_outlined(ui,"取消",true,th).clicked() { close = true; }
                    });
                });
        }
        if save {
            if let Some(realm) = self.wizard_realm_dialog.take() {
                self.wizard_proj.custom_realms.push(realm);
                self.wizard_proj.custom_realms.sort_by_key(|r| r.order);
            }
        }
        if close { self.wizard_realm_dialog = None; }
    }
}

// ═══════════════════════════════════════════════════════
//  字体
// ═══════════════════════════════════════════════════════
pub fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let paths: &[&str] = if cfg!(target_os = "macos") {
        &["/System/Library/Fonts/PingFang.ttc","/System/Library/Fonts/STHeiti Light.ttc","/Library/Fonts/Arial Unicode MS.ttf"]
    } else if cfg!(target_os = "windows") {
        &["C:\\Windows\\Fonts\\msyh.ttc","C:\\Windows\\Fonts\\simsun.ttc"]
    } else { &["/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc"] };
    for &p in paths {
        if let Ok(b) = std::fs::read(p) {
            fonts.font_data.insert("cjk".into(), egui::FontData::from_owned(b));
            for fam in [&egui::FontFamily::Proportional, &egui::FontFamily::Monospace] {
                fonts.families.get_mut(fam).unwrap().insert(0,"cjk".into());
            }
            break;
        }
    }
    ctx.set_fonts(fonts);
}

// ═══════════════════════════════════════════════════════
//  工具函数
// ═══════════════════════════════════════════════════════
fn decode_image(bytes: &[u8]) -> Result<egui::ColorImage, String> {
    let img  = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
    let rgba = img.to_rgba8();
    let [w,h] = [rgba.width() as usize, rgba.height() as usize];
    let pixels: Vec<Color32> = rgba.pixels().map(|p| Color32::from_rgba_unmultiplied(p[0],p[1],p[2],p[3])).collect();
    Ok(egui::ColorImage { size:[w,h], pixels })
}

/// 解码 GIF，返回所有帧 + 每帧延迟（毫秒）
fn decode_gif(bytes: &[u8]) -> Result<(Vec<egui::ColorImage>, Vec<u32>), String> {
    use image::AnimationDecoder;
    let cursor = std::io::Cursor::new(bytes);
    let decoder = image::codecs::gif::GifDecoder::new(cursor).map_err(|e| e.to_string())?;
    let mut images = Vec::new();
    let mut delays = Vec::new();
    for f in decoder.into_frames() {
        let frame = f.map_err(|e| e.to_string())?;
        let (num, den) = frame.delay().numer_denom_ms();
        let ms = if den == 0 { 100 } else { num / den.max(1) };
        let buf = frame.buffer();
        let [w, h] = [buf.width() as usize, buf.height() as usize];
        let pixels: Vec<Color32> = buf.pixels().map(|p| Color32::from_rgba_unmultiplied(p[0],p[1],p[2],p[3])).collect();
        images.push(egui::ColorImage { size:[w,h], pixels });
        delays.push(ms.max(20));
    }
    if images.is_empty() { return Err("GIF 无可用帧".into()); }
    Ok((images, delays))
}

/// 生成首帧的高斯模糊版本，用于毛玻璃背板。
fn decode_blurred(bytes: &[u8]) -> Result<egui::ColorImage, String> {
    let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
    // 缩小再模糊，远快于直接对大图模糊
    let small = img.resize_exact(
        img.width().min(320).max(64),
        ((img.height() as f32 / img.width() as f32) * img.width().min(320).max(64) as f32) as u32,
        image::imageops::FilterType::Triangle,
    );
    let blurred = image::imageops::blur(&small.to_rgba8(), 18.0);
    let [w, h] = [blurred.width() as usize, blurred.height() as usize];
    let pixels: Vec<Color32> = blurred.pixels().map(|p| Color32::from_rgba_unmultiplied(p[0],p[1],p[2],p[3])).collect();
    Ok(egui::ColorImage { size:[w,h], pixels })
}

/// 在指定 `rect` 区域绘制毛玻璃背板（截取屏幕级模糊图的对应 UV 区域并加深色叠加层与高光描边）。
fn paint_frosted_at(ui: &egui::Ui, slot: egui::layers::ShapeIdx, rect: Rect, rounding: Rounding, blurred: &egui::TextureHandle, th: Th) {
    let screen = ui.ctx().screen_rect();
    let sw = screen.width().max(1.0);
    let sh = screen.height().max(1.0);
    let uv = Rect::from_min_max(
        pos2((rect.min.x - screen.min.x) / sw, (rect.min.y - screen.min.y) / sh),
        pos2((rect.max.x - screen.min.x) / sw, (rect.max.y - screen.min.y) / sh),
    );
    let tint = if th.dark { ca(8,10,20,150) } else { ca(255,255,255,160) };
    ui.painter().set(slot, Shape::Vec(vec![
        Shape::image(blurred.id(), rect, uv, Color32::WHITE),
        Shape::rect_filled(rect, rounding, tint),
        Shape::rect_stroke(rect, rounding, Stroke::new(1.0, ca(255,255,255,60))),
    ]));
}

/// `card()` 的变体：若提供了模糊纹理则绘制毛玻璃背板，否则退回普通卡片。
fn card_or_frosted<R>(
    ui: &mut egui::Ui, th: Th, r: Rounding, pad: Margin,
    blurred: Option<&egui::TextureHandle>,
    f: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    if let Some(blur) = blurred {
        let slot = ui.painter().add(Shape::Noop);
        let inner = egui::Frame::none().fill(Color32::TRANSPARENT).rounding(r).inner_margin(pad).show(ui, f);
        paint_frosted_at(ui, slot, inner.response.rect, r, blur, th);
        inner
    } else {
        card(ui, th, r, pad, f)
    }
}
fn fmt_num(n: usize) -> String { if n>=10_000 { format!("{:.1}万",n as f32/10_000.0) } else { n.to_string() } }
fn truncate_chars(s: &str, max: usize) -> String {
    let mut out = String::with_capacity(max+3); let mut cnt=0;
    for ch in s.chars() { if cnt>=max { out.push('…'); break; } out.push(ch); cnt+=1; }
    out
}
