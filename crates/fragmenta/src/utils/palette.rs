//! 取色盘（note-taxonomy-form spec）：洗牌袋 + 名字 → 颜色映射。
//!
//! 应用层纯逻辑模块（仅用颜色数据类型，零 UI 组件依赖）：
//! - 6 色 Material Design 取色盘（六个不同 hue 家族、同一 500 亮度档）；
//! - 洗牌袋分配：每个首次出现的名字从袋中不放回取一色，袋空后重洗重置；
//! - 名字 → 颜色映射进程内稳定（同屏一致），不持久化（重启后重新随机分配）；
//! - 分类与标签共用一个键空间（键 = 名字字符串，同名同色）。
//!
//! 随机源：标准库随机种子 hasher（`RandomState`）对 `(seed, counter)` 哈希
//! 驱动 Fisher-Yates。`RandomState` 的密钥在进程启动时随机生成、进程内固定，
//! 因此同进程内洗牌稳定、跨进程随机；无密码学强度需求，不引入新依赖。

use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash, Hasher};

use gpui_kit::{Hsla, hsla, rgb};

/// 初始洗牌种子串：`RandomState` 每进程随机，固定串的哈希结果仍随进程而变。
const INIT_SEED: &str = "fragmenta-palette-init";

/// 纯空白名字的中性回退色（中灰）：不参与洗牌袋分配，也不落映射。
fn neutral() -> Hsla {
    hsla(0., 0., 0.5, 1.)
}

/// 色盘色的填充 Tag 前景（白）：色盘统一 500 亮度档，白字深 / 浅主题下均可读。
pub(crate) fn filled_foreground() -> Hsla {
    hsla(0., 0., 1., 1.)
}

/// 6 色 Material Design 取色盘：六个不同 hue 家族、同一 500 亮度档。
fn palette_colors() -> [Hsla; 6] {
    [
        rgb(0xF44336).into(), // Red 500
        rgb(0xFF9800).into(), // Orange 500
        rgb(0x4CAF50).into(), // Green 500
        rgb(0x009688).into(), // Teal 500
        rgb(0x2196F3).into(), // Blue 500
        rgb(0x9C27B0).into(), // Purple 500
    ]
}

/// 名字取色器：洗牌袋分配 + 进程内稳定映射。
pub struct ColorPalette {
    /// 袋中剩余颜色（已洗牌；取空即重洗重置）。
    bag: Vec<Hsla>,
    /// 名字 → 颜色（进程生命周期内稳定）。
    map: HashMap<String, Hsla>,
    /// 洗牌随机源（进程内固定，跨进程随机）。
    hasher: RandomState,
}

impl Default for ColorPalette {
    fn default() -> Self {
        Self::new()
    }
}

impl ColorPalette {
    /// 创建取色器：初始袋装满 6 色并洗牌。
    pub fn new() -> Self {
        let mut bag = palette_colors().to_vec();
        let hasher = RandomState::new();
        shuffle(&mut bag, INIT_SEED, &hasher);
        Self {
            bag,
            map: HashMap::new(),
            hasher,
        }
    }

    /// 按名字取色：首次出现的名字从袋中不放回取一色并缓存；
    /// 袋空后重洗重置（一轮之内 6 色互不重复，用光后重来一轮）。
    ///
    /// 纯空白名字返回中性回退色，不参与分配。
    pub fn color_for(&mut self, name: &str) -> Hsla {
        let key = name.trim();
        if key.is_empty() {
            return neutral();
        }
        if let Some(color) = self.map.get(key) {
            return *color;
        }
        if self.bag.is_empty() {
            self.bag = palette_colors().to_vec();
            shuffle(&mut self.bag, key, &self.hasher);
        }
        let color = self.bag.pop().expect("袋空重置后必有余色");
        self.map.insert(key.to_string(), color);
        color
    }
}

/// Fisher-Yates 洗牌：每个随机数为 `hasher` 对 `(seed, counter)` 的哈希。
fn shuffle(bag: &mut [Hsla], seed: &str, hasher: &RandomState) {
    let mut counter: u64 = 0;
    for i in (1..bag.len()).rev() {
        let mut h = hasher.build_hasher();
        (seed, counter).hash(&mut h);
        counter += 1;
        let j = (h.finish() % (i as u64 + 1)) as usize;
        bag.swap(i, j);
    }
}
