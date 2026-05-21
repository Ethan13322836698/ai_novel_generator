use crate::novel::CustomRealm;

/// 把用户填写的 sub_levels 规范化：接受 `/`、`／`、`→` 任一作为分隔符，统一展示为 `→`。
pub fn normalize_sub_levels(s: &str) -> String {
    s.split(|c: char| c == '/' || c == '／' || c == '→')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("→")
}

pub fn build_custom_realms_prompt(realms: &[CustomRealm]) -> String {
    if realms.is_empty() { return String::new(); }
    let mut list: Vec<&CustomRealm> = realms.iter().collect();
    list.sort_by_key(|r| r.order);
    let mut out = String::from("\n\n【自定义境界体系】\n");
    for (i, r) in list.iter().enumerate() {
        let desc = if r.description.is_empty() { String::new() } else { format!("（{}）", r.description) };
        out.push_str(&format!("{}. {}{}\n", i + 1, r.name, desc));
        if !r.sub_levels.is_empty() {
            out.push_str(&format!("   小境界：{}\n", normalize_sub_levels(&r.sub_levels)));
        }
    }
    out
}

/// 境界/力量体系定义
#[derive(Debug, Clone)]
pub struct RealmSystem {
    pub id:          &'static str,
    pub name:        &'static str,
    pub icon:        &'static str,
    pub category:    &'static str,      // 分类：修炼 / 武道 / 魔法 / 科幻 / 其他
    pub levels:      &'static str,      // 境界链，用 → 分隔
    pub description: &'static str,
}

pub fn all_realms() -> &'static [RealmSystem] {
    &REALMS
}

pub fn realm_by_id(id: &str) -> Option<&'static RealmSystem> {
    REALMS.iter().find(|r| r.id == id)
}

/// 将选中的境界列表格式化成可插入 Prompt 的说明段落
pub fn build_realm_prompt(ids: &[String]) -> String {
    if ids.is_empty() {
        return String::new();
    }
    let mut out = String::from("\n\n【当前世界境界体系】\n");
    for id in ids {
        if let Some(r) = realm_by_id(id) {
            out.push_str(&format!("▸ {}（{}）：{}\n", r.name, r.icon, r.levels));
        }
    }
    out.push_str("请在行文中自然体现以上境界体系，角色实力对比须符合境界逻辑。");
    out
}

static REALMS: &[RealmSystem] = &[
    // ── 修炼类 ─────────────────────────────────────────────────────────────
    RealmSystem {
        id: "xianxia_classic",
        name: "仙道经典",
        icon: "⚡",
        category: "修炼",
        levels: "练气→筑基→金丹→元婴→化神→炼虚→合体→大乘→渡劫→飞升",
        description: "最经典修仙境界，符合传统仙侠体系",
    },
    RealmSystem {
        id: "xianxia_extended",
        name: "仙道扩展",
        icon: "🌌",
        category: "修炼",
        levels: "淬体→练气→筑基→金丹→元婴→化神→炼虚→合体→大乘→渡劫→飞升→太乙→金仙→大罗→混元",
        description: "扩展版修仙境界，涵盖仙界层次",
    },
    RealmSystem {
        id: "dao_realm",
        name: "道境层次",
        icon: "☯️",
        category: "修炼",
        levels: "凡境→道基→道种→道心→道躯→道魂→道尊→道圣→道帝→道祖→天道",
        description: "以「道」为核心的修炼层次",
    },
    RealmSystem {
        id: "demon_path",
        name: "魔道境界",
        icon: "🔥",
        category: "修炼",
        levels: "炼皮→炼骨→炼脉→炼脏→炼神→魔徒→魔师→魔王→魔皇→魔圣→魔祖",
        description: "走魔道修炼的境界体系",
    },
    RealmSystem {
        id: "body_refine",
        name: "炼体境界",
        icon: "💪",
        category: "修炼",
        levels: "淬骨→铸筋→凝血→炼脏→金身→铜躯→铁甲→玄铁→天罡→不灭→神体",
        description: "以肉身修炼为主的体修体系",
    },
    RealmSystem {
        id: "spirit_root",
        name: "灵根品阶",
        icon: "🌱",
        category: "修炼",
        levels: "废灵根→七灵根→六灵根→五灵根→四灵根→三灵根→双灵根→单灵根→变异灵根→天灵根→混沌灵根",
        description: "灵根品质决定修炼天赋的辅助体系",
    },

    // ── 武道类 ─────────────────────────────────────────────────────────────
    RealmSystem {
        id: "wudao_classic",
        name: "武道经典",
        icon: "🥊",
        category: "武道",
        levels: "后天→先天→宗师→武圣→武神→武尊→武帝→武皇→武宗→武祖",
        description: "经典武侠/武道境界体系",
    },
    RealmSystem {
        id: "wudao_modern",
        name: "武道现代",
        icon: "⚔️",
        category: "武道",
        levels: "炼体期→炼气期→化劲期→先天期→宗师期→大宗师→武王→武皇→武帝→武圣",
        description: "融合现代元素的武道体系",
    },
    RealmSystem {
        id: "martial_god",
        name: "武神境界",
        icon: "🗡️",
        category: "武道",
        levels: "武徒→武者→武师→武灵→武将→武侯→武王→武帝→半步武神→武神→武祖",
        description: "以武神为终极目标的体系",
    },
    RealmSystem {
        id: "intent_realm",
        name: "武意境界",
        icon: "🌊",
        category: "武道",
        levels: "入门→小成→大成→炉火纯青→登峰造极→意境初开→意境小成→意境大成→意境化神→天人合一",
        description: "以武意/武道感悟为核心的进阶体系",
    },

    // ── 玄幻/魔法类 ────────────────────────────────────────────────────────
    RealmSystem {
        id: "dou_qi",
        name: "斗气境界",
        icon: "🐲",
        category: "玄幻",
        levels: "斗者→斗师→大斗师→斗灵→斗王→斗皇→斗宗→斗尊→斗圣→斗帝",
        description: "《斗破苍穹》式斗气体系",
    },
    RealmSystem {
        id: "magic_star",
        name: "魔法星级",
        icon: "✨",
        category: "玄幻",
        levels: "一星→二星→三星→四星→五星→六星→七星→八星→九星→神级",
        description: "以星级划分魔法师实力的体系",
    },
    RealmSystem {
        id: "divine_realm",
        name: "神魔体系",
        icon: "⚜️",
        category: "玄幻",
        levels: "凡人→武者→武师→武将→武侯→武王→武帝→半神→神→主神→古神→混沌神",
        description: "神魔争霸世界的力量层级",
    },
    RealmSystem {
        id: "bloodline",
        name: "血脉等阶",
        icon: "🩸",
        category: "玄幻",
        levels: "凡血→黄阶→玄阶→地阶→天阶→神阶→祖血→始血→混沌血脉",
        description: "以血脉觉醒和进化为核心的体系",
    },
    RealmSystem {
        id: "rune_grade",
        name: "符文品级",
        icon: "📜",
        category: "玄幻",
        levels: "一品符文→二品→三品→四品→五品→六品→七品→八品→九品→神品→传说→神话",
        description: "以符文等级划分力量的奇幻体系",
    },

    // ── 都市/异能类 ────────────────────────────────────────────────────────
    RealmSystem {
        id: "ability_level",
        name: "异能等级",
        icon: "⚡",
        category: "异能",
        levels: "觉醒→F级→E级→D级→C级→B级→A级→S级→SS级→SSS级→神级",
        description: "都市异能/进化者的能力等级",
    },
    RealmSystem {
        id: "awakener",
        name: "觉醒者阶段",
        icon: "🔮",
        category: "异能",
        levels: "潜力者→初阶觉醒→中阶觉醒→高阶觉醒→精英觉醒→超凡觉醒→传说觉醒→神话觉醒",
        description: "以觉醒阶段划分超能力者实力",
    },

    // ── 末世类 ─────────────────────────────────────────────────────────────
    RealmSystem {
        id: "apocalypse_rank",
        name: "末世进化",
        icon: "☠️",
        category: "末世",
        levels: "普通人→一阶→二阶→三阶→四阶→五阶→六阶→超阶→神阶→变异神阶",
        description: "末世中人类进化的阶段体系",
    },
    RealmSystem {
        id: "zombie_rank",
        name: "丧尸进化",
        icon: "🧟",
        category: "末世",
        levels: "普通丧尸→二阶丧尸→三阶变种→四阶精英→五阶领主→六阶王者→BOSS级→神级丧尸",
        description: "末世丧尸的进化等级，形成反向威胁体系",
    },

    // ── 科幻类 ─────────────────────────────────────────────────────────────
    RealmSystem {
        id: "civilization_tier",
        name: "文明等级",
        icon: "🚀",
        category: "科幻",
        levels: "Ⅰ型文明→Ⅱ型文明→Ⅲ型文明→Ⅳ型文明（星系）→Ⅴ型（星域）→Ⅵ型（宇宙）→超维文明",
        description: "卡尔达舍夫文明等级量表",
    },
    RealmSystem {
        id: "mecha_rank",
        name: "机甲等级",
        icon: "🤖",
        category: "科幻",
        levels: "E级机甲→D级→C级→B级→A级→S级→神机→传说机甲→星灵机甲",
        description: "星际机甲/战甲的等级体系",
    },
    RealmSystem {
        id: "gene_evolution",
        name: "基因进化",
        icon: "🧬",
        category: "科幻",
        levels: "原始人类→基因改造→一阶进化→二阶→三阶→四阶→五阶→超进化→神进化→宇宙生命体",
        description: "以基因改造和进化为核心的科幻体系",
    },

    // ── 系统类 ─────────────────────────────────────────────────────────────
    RealmSystem {
        id: "system_grade",
        name: "系统等级",
        icon: "💻",
        category: "系统",
        levels: "Lv.1→Lv.10→Lv.20→Lv.30→Lv.50→Lv.100→隐藏职业→传说职业→神话职业→系统主宰",
        description: "系统流中人物等级与职业体系",
    },
    RealmSystem {
        id: "infinite_rank",
        name: "无限副本",
        icon: "🌀",
        category: "无限",
        levels: "新手→普通玩家→精英→王者→传说→神话→永恒→起源→无上",
        description: "无限流副本中玩家的积分与实力等级",
    },
];
