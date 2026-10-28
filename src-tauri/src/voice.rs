//! 语音（v2.1 P14，DECISIONS #52）：她说话时出声。**默认关闭。**
//!
//! - 只读精简后的部分（`condense`：去掉括号里的动作、表情符号、网址和代码，取开头一两句，默认 40 字以内）。
//! - 引擎（可扩展，新增一个就是 `synth` 里多一个分支 + 设置页多一张卡片）：
//!   - `system`：Windows 自带的语音（浏览器的 Web Speech，免费、离线；声音不像她，但开箱就能用）；
//!   - `fish`：Fish Audio（云端，社区里有现成的七海千秋声音模型；有免费档 `s2.1-pro-free`）；
//!   - `sovits`：GPT-SoVITS（用户自己在本机运行的服务，`api_v2.py` 的 `/tts`）；
//!   - `openai`：OpenAI 兼容的 `/audio/speech` 接口。
//! - 云端 / 本机服务合成的音频缓存在 `%APPDATA%\DeskMate\voice-cache\`（同一句话第二次不再合成；有大小上限，旧的先删）。
//!   音频由桌宠窗口播放（`voice-play` 事件 → `voice_audio` 取字节），系统语音直接在桌宠窗口里合成（`voice-say`）。
//! - Key 只存系统凭据管理器（`backend:tts-fish` / `backend:tts-openai`），不进设置、不进日志、不给前端。
//! - 什么时候说：小反应、主动说话、聊天回复、小游戏，各自可以关；勿扰时主动说话不出声；全屏时默认不出声（可选只用声音说）。

use crate::settings::Settings;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

pub const ENGINES: [&str; 4] = ["system", "fish", "sovits", "openai"];
/// Fish Audio 上现成的七海千秋声音（社区模型，模型页 https://fish.audio/m/<id>）
pub const FISH_PRESETS: [(&str, &str); 2] = [
    (
        "d5e032724f8149629ec6d03735ec4c85",
        "七海千秋（日语配音风格）",
    ),
    (
        "3d3bed2283b249a684cd8cd79dafe3e8",
        "七海千秋（英语配音风格）",
    ),
];
pub const FISH_KEY_ID: &str = "tts-fish";
pub const OPENAI_KEY_ID: &str = "tts-openai";
/// 缓存上限
const CACHE_MAX_BYTES: u64 = 40 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoiceScenes {
    /// 摸头、拖动这些小反应
    pub reactions: bool,
    /// 她主动说的话（播报、提醒、关怀、看画面……）
    pub proactive: bool,
    /// 聊天回复
    pub chat: bool,
    /// 小游戏里的台词
    pub games: bool,
}

impl Default for VoiceScenes {
    fn default() -> Self {
        Self {
            reactions: true,
            proactive: true,
            chat: true,
            games: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoiceSettings {
    pub enabled: bool,
    /// 空 = 还没选（头顶菜单不显示声音开关）
    pub engine: String,
    /// 0~100
    pub volume: u8,
    /// 每次最多读多少字
    pub max_chars: u32,
    /// 语速（0.5~2）
    pub speed: f32,
    pub scenes: VoiceScenes,
    /// 全屏时（比如在玩全屏游戏）只用声音说主动的话（默认不出声）
    pub fullscreen_voice: bool,
    /// 云端引擎每天最多合成多少字（防止花超）
    pub daily_chars: u32,
    /// 系统语音：声音名字（空 = 自动挑中文）、音调（0~2，默认偏高一点）
    pub system_voice: String,
    pub system_pitch: f32,
    /// Fish Audio
    pub fish_url: String,
    pub fish_model: String,
    /// Fish 的合成模型：s2.1-pro-free（免费）/ s2.1-pro / s1……
    pub fish_engine: String,
    /// GPT-SoVITS（本机服务）
    pub sovits_url: String,
    pub sovits_ref_audio: String,
    pub sovits_ref_text: String,
    pub sovits_ref_lang: String,
    /// OpenAI 兼容
    pub openai_url: String,
    pub openai_model: String,
    pub openai_voice: String,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            engine: String::new(),
            volume: 80,
            max_chars: 40,
            speed: 1.0,
            scenes: VoiceScenes::default(),
            fullscreen_voice: false,
            daily_chars: 5000,
            system_voice: String::new(),
            system_pitch: 1.25,
            fish_url: "https://api.fish.audio".into(),
            fish_model: FISH_PRESETS[0].0.into(),
            fish_engine: "s2.1-pro-free".into(),
            sovits_url: "http://127.0.0.1:9880".into(),
            sovits_ref_audio: String::new(),
            sovits_ref_text: String::new(),
            sovits_ref_lang: "ja".into(),
            openai_url: "https://api.openai.com/v1".into(),
            openai_model: "gpt-4o-mini-tts".into(),
            openai_voice: "nova".into(),
        }
    }
}

fn clip(s: &str, n: usize) -> String {
    s.trim().chars().take(n).collect()
}

/// 只允许 https，或者本机的 http（GPT-SoVITS 这类本机服务）
pub fn url_ok(u: &str) -> bool {
    u.starts_with("https://")
        || u.starts_with("http://127.0.0.1")
        || u.starts_with("http://localhost")
}

impl VoiceSettings {
    pub fn sanitize(mut self) -> Self {
        if !self.engine.is_empty() && !ENGINES.contains(&self.engine.as_str()) {
            self.engine = String::new();
        }
        if self.engine.is_empty() {
            self.enabled = false;
        }
        self.volume = self.volume.min(100);
        self.max_chars = self.max_chars.clamp(10, 120);
        self.daily_chars = self.daily_chars.min(100_000);
        if !self.speed.is_finite() {
            self.speed = 1.0;
        }
        self.speed = self.speed.clamp(0.5, 2.0);
        if !self.system_pitch.is_finite() {
            self.system_pitch = 1.25;
        }
        self.system_pitch = self.system_pitch.clamp(0.0, 2.0);
        self.system_voice = clip(&self.system_voice, 100);
        let d = VoiceSettings::default();
        for (u, def) in [
            (&mut self.fish_url, &d.fish_url),
            (&mut self.sovits_url, &d.sovits_url),
            (&mut self.openai_url, &d.openai_url),
        ] {
            *u = clip(u, 200).trim_end_matches('/').to_string();
            if !url_ok(u) {
                *u = def.clone();
            }
        }
        self.fish_model = clip(&self.fish_model, 64)
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();
        self.fish_engine = clip(&self.fish_engine, 32)
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
            .collect();
        self.sovits_ref_audio = clip(&self.sovits_ref_audio, 400);
        self.sovits_ref_text = clip(&self.sovits_ref_text, 200);
        self.sovits_ref_lang = clip(&self.sovits_ref_lang, 8);
        self.openai_model = clip(&self.openai_model, 64);
        self.openai_voice = clip(&self.openai_voice, 32);
        self
    }

    /// 要不要花钱（云端、按字计费的）
    fn metered(&self) -> bool {
        match self.engine.as_str() {
            "fish" => !self.fish_engine.ends_with("-free"),
            "openai" => true,
            _ => false,
        }
    }
}

/// 什么场合说的
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scene {
    Reaction,
    Proactive,
    Chat,
    Game,
}

impl Scene {
    fn on(self, s: &VoiceScenes) -> bool {
        match self {
            Scene::Reaction => s.reactions,
            Scene::Proactive => s.proactive,
            Scene::Chat => s.chat,
            Scene::Game => s.games,
        }
    }
}

fn is_emoji(c: char) -> bool {
    matches!(c as u32,
        0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2B00..=0x2BFF | 0xFE0F | 0x200D)
}

/// 只读精简后的部分（纯函数）：去掉括号里的动作和旁白、表情符号、网址、代码，取开头的句子，最多 `max` 个字
pub fn condense(text: &str, max: usize) -> String {
    // 去掉代码块
    let mut t = String::new();
    let mut in_code = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if !in_code {
            t.push_str(line);
            t.push('\n');
        }
    }
    // 去掉括号里的内容（（动作）、(…)、【…】、*…*）、网址、表情
    let mut out = String::new();
    let mut depth = 0i32;
    let mut star = false;
    for w in t
        .split_whitespace()
        .filter(|w| !w.starts_with("http://") && !w.starts_with("https://"))
    {
        let mut word = String::new();
        for c in w.chars() {
            match c {
                '（' | '(' | '【' | '[' => depth += 1,
                '）' | ')' | '】' | ']' => depth = (depth - 1).max(0),
                '*' => star = !star,
                _ if depth > 0 || star || is_emoji(c) || c == '`' || c == '#' => {}
                _ => word.push(c),
            }
        }
        if word.is_empty() {
            continue;
        }
        // 只在两个英文单词之间留空格（中文之间的换行、空格去掉）
        if out.ends_with(|c: char| c.is_ascii_alphanumeric())
            && word.starts_with(|c: char| c.is_ascii_alphanumeric())
        {
            out.push(' ');
        }
        out.push_str(&word);
    }
    let out = out.trim();
    // 按句子取，直到超过上限
    let mut result = String::new();
    let mut count = 0usize;
    let mut cur = String::new();
    for c in out.chars() {
        cur.push(c);
        if matches!(c, '。' | '！' | '？' | '!' | '?' | '…' | '～' | '~' | '\n') {
            let n = cur.chars().count();
            if count > 0 && count + n > max {
                return result.trim().to_string();
            }
            result.push_str(&cur);
            count += n;
            cur.clear();
            if count >= max {
                break;
            }
        }
    }
    if result.is_empty() || (count < max && !cur.is_empty() && count + cur.chars().count() <= max) {
        result.push_str(&cur);
    }
    let r = result.trim();
    if r.chars().count() > max {
        // 第一句就太长：在最后一个逗号处截断，没有逗号就硬截
        let cut: String = r.chars().take(max).collect();
        if let Some(i) = cut.rfind(['，', ',', '、', '；']) {
            return cut[..i].to_string();
        }
        return cut;
    }
    r.to_string()
}

#[derive(Default)]
pub struct VoiceState {
    /// （日期，今天合成了多少字）
    chars: Mutex<(String, u32)>,
    /// 上一句小反应的时间（毫秒），小反应 1.5 秒内只说一句
    last_reaction: Mutex<i64>,
}

/// 头顶气泡菜单里「声音」开关的状态：None = 还没选引擎，不显示这个气泡
pub fn menu_state(s: &Settings) -> Option<bool> {
    (!s.voice.engine.is_empty()).then_some(s.voice.enabled)
}

/// 切换声音开关，返回切换后的状态
pub fn toggle(app: &AppHandle) -> bool {
    let st = app.state::<AppState>();
    let on = !st.settings().voice.enabled;
    let saved = st.update_settings(|s| s.voice.enabled = on).voice.enabled;
    if !saved {
        let _ = app.emit_to("pet", "voice-stop", ());
    }
    crate::desktop::broadcast_settings_changed(app);
    saved
}

/// 系统语音：让桌宠窗口自己合成
#[derive(Debug, Clone, Serialize)]
pub struct SayEvent {
    pub text: String,
    pub voice: String,
    pub rate: f32,
    pub pitch: f32,
    pub volume: f32,
}

/// 合成好的音频：桌宠窗口用 `voice_audio(id)` 取来播放
#[derive(Debug, Clone, Serialize)]
pub struct PlayEvent {
    pub id: String,
    pub volume: f32,
}

/// 现在能不能出声（纯函数）
pub fn should_speak(s: &VoiceSettings, scene: Scene, dnd: bool, fullscreen: bool) -> bool {
    s.enabled
        && !s.engine.is_empty()
        && scene.on(&s.scenes)
        && !(dnd && scene == Scene::Proactive)
        && (!fullscreen || (s.fullscreen_voice && scene == Scene::Proactive))
}

/// 她说了一句话：按设置决定要不要读出来（不等结果）
pub fn on_line(app: &AppHandle, text: &str, scene: Scene) {
    let st = app.state::<AppState>();
    let s = st.settings();
    if !should_speak(&s.voice, scene, s.dnd, st.daily.is_fullscreen()) {
        return;
    }
    if scene == Scene::Reaction {
        let now = crate::desktop::now_ms();
        let mut last = st
            .voice
            .last_reaction
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if now - *last < 1500 {
            return;
        }
        *last = now;
    }
    let text = condense(text, s.voice.max_chars as usize);
    if text.is_empty() {
        return;
    }
    speak(app, s.voice, text);
}

/// 读一句（已经精简过的）
pub fn speak(app: &AppHandle, v: VoiceSettings, text: String) {
    let volume = v.volume as f32 / 100.0;
    if v.engine == "system" {
        let _ = app.emit_to(
            "pet",
            "voice-say",
            SayEvent {
                text,
                voice: v.system_voice.clone(),
                rate: v.speed,
                pitch: v.system_pitch,
                volume,
            },
        );
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let st = app.state::<AppState>();
        match synth_cached(&st, &v, &text).await {
            Ok(id) => {
                let _ = app.emit_to("pet", "voice-play", PlayEvent { id, volume });
            }
            Err(e) => log::info!("语音合成失败：{e}"),
        }
    });
}

fn cache_dir(st: &AppState) -> std::path::PathBuf {
    st.paths.root().join("voice-cache")
}

/// 缓存文件名：引擎参数 + 文字的哈希（只含十六进制）
pub fn cache_id(v: &VoiceSettings, text: &str) -> String {
    use sha2::Digest;
    let key = match v.engine.as_str() {
        "fish" => format!(
            "fish|{}|{}|{}|{}",
            v.fish_model, v.fish_engine, v.speed, text
        ),
        "sovits" => format!(
            "sovits|{}|{}|{}|{}",
            v.sovits_ref_audio, v.sovits_ref_lang, v.speed, text
        ),
        "openai" => format!(
            "openai|{}|{}|{}|{}",
            v.openai_model, v.openai_voice, v.speed, text
        ),
        other => format!("{other}|{text}"),
    };
    let d = sha2::Sha256::digest(key.as_bytes());
    d.iter().take(16).map(|b| format!("{b:02x}")).collect()
}

/// 合成（有缓存就直接用），返回缓存 id
async fn synth_cached(st: &AppState, v: &VoiceSettings, text: &str) -> Result<String, String> {
    let id = cache_id(v, text);
    let dir = cache_dir(st);
    let path = dir.join(format!("{id}.audio"));
    if path.is_file() {
        // 更新修改时间（最近用过的不会先被删）
        let _ = std::fs::File::options()
            .append(true)
            .open(&path)
            .and_then(|f| f.set_modified(std::time::SystemTime::now()));
        return Ok(id);
    }
    if v.metered() {
        let (day, _) = crate::usage::today();
        let mut c = st.voice.chars.lock().unwrap_or_else(|e| e.into_inner());
        if c.0 != day {
            *c = (day, 0);
        }
        let n = text.chars().count() as u32;
        if c.1 + n > v.daily_chars {
            return Err("今天的语音字数用完了".into());
        }
        c.1 += n;
    }
    let bytes = synth(st, v, text).await?;
    if bytes.len() < 64 {
        return Err("合成出来的音频是空的".into());
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    trim_cache(&dir);
    Ok(id)
}

/// 缓存超过上限时，从最久没用的开始删
fn trim_cache(dir: &std::path::Path) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, u64, std::path::PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            if !(m.is_file() && e.path().extension().is_some_and(|x| x == "audio")) {
                return None;
            }
            Some((m.modified().ok()?, m.len(), e.path()))
        })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    files.sort_by_key(|f| f.0);
    for (_, len, p) in files {
        if total <= CACHE_MAX_BYTES {
            break;
        }
        if std::fs::remove_file(&p).is_ok() {
            total -= len;
        }
    }
}

/// 读缓存里的音频（只读 voice-cache 文件夹里、名字是 32 位十六进制的文件）
pub fn read_cached(st: &AppState, id: &str) -> Result<Vec<u8>, String> {
    if id.len() != 32 || !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("不对的音频编号".into());
    }
    let p = cache_dir(st).join(format!("{id}.audio"));
    let meta = std::fs::symlink_metadata(&p).map_err(|_| "找不到音频".to_string())?;
    if !meta.is_file() || meta.len() > 20 * 1024 * 1024 {
        return Err("音频文件不对".into());
    }
    std::fs::read(&p).map_err(|e| e.to_string())
}

/// 清空缓存
pub fn clear_cache(st: &AppState) -> u64 {
    let dir = cache_dir(st);
    let mut freed = 0;
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            if e.path().extension().is_some_and(|x| x == "audio")
                && let Ok(m) = e.metadata()
                && std::fs::remove_file(e.path()).is_ok()
            {
                freed += m.len();
            }
        }
    }
    freed
}

fn key(st: &AppState, id: &str) -> Result<String, String> {
    st.secrets
        .get(id)?
        .filter(|k| !k.is_empty())
        .ok_or_else(|| "还没填语音服务的 Key：到「设置 → 声音」里填。".to_string())
}

/// 请求体（纯函数，方便测试）
pub fn request_body(v: &VoiceSettings, text: &str) -> serde_json::Value {
    match v.engine.as_str() {
        "fish" => serde_json::json!({
            "text": text,
            "reference_id": v.fish_model,
            "format": "mp3",
            "mp3_bitrate": 64,
            "latency": "balanced",
            "normalize": true,
            "prosody": { "speed": v.speed },
        }),
        "sovits" => serde_json::json!({
            "text": text,
            "text_lang": "zh",
            "ref_audio_path": v.sovits_ref_audio,
            "prompt_text": v.sovits_ref_text,
            "prompt_lang": v.sovits_ref_lang,
            "media_type": "wav",
            "streaming_mode": false,
            "speed_factor": v.speed,
        }),
        _ => serde_json::json!({
            "model": v.openai_model,
            "voice": v.openai_voice,
            "input": text,
            "response_format": "mp3",
            "speed": v.speed,
        }),
    }
}

/// 调用引擎合成
async fn synth(st: &AppState, v: &VoiceSettings, text: &str) -> Result<Vec<u8>, String> {
    let body = request_body(v, text);
    let req = match v.engine.as_str() {
        "fish" => st
            .http
            .post(format!("{}/v1/tts", v.fish_url))
            .bearer_auth(key(st, FISH_KEY_ID)?)
            .header("model", &v.fish_engine),
        "sovits" => {
            if v.sovits_ref_audio.is_empty() {
                return Err("GPT-SoVITS 还没填参考音频".into());
            }
            st.http.post(format!("{}/tts", v.sovits_url))
        }
        "openai" => st
            .http
            .post(format!("{}/audio/speech", v.openai_url))
            .bearer_auth(key(st, OPENAI_KEY_ID)?),
        _ => return Err("这个引擎不用合成".into()),
    };
    let timeout = if v.engine == "sovits" { 90 } else { 30 };
    let resp = req
        .json(&body)
        .timeout(std::time::Duration::from_secs(timeout))
        .send()
        .await
        .map_err(|e| {
            format!(
                "连不上语音服务：{}",
                crate::redact::redact_secrets(&e.to_string())
            )
        })?;
    let status = resp.status();
    if !status.is_success() {
        let t = resp.text().await.unwrap_or_default();
        let msg = crate::backends::http::error_message(&t);
        return Err(match status.as_u16() {
            401 | 403 => "语音服务的 Key 不对或没有权限".into(),
            402 => "语音服务的余额不够了（可以换成免费的模型）".into(),
            429 => "语音服务太忙了，等一会儿再试".into(),
            s => format!("语音服务返回了 {s}：{msg}"),
        });
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() > 20 * 1024 * 1024 {
        return Err("音频太大了".into());
    }
    Ok(bytes.to_vec())
}

/// 设置页「试听」：返回错误说明（成功时由桌宠窗口出声）
pub async fn test(app: &AppHandle) -> Result<(), String> {
    let st = app.state::<AppState>();
    let v = st.settings().voice;
    if v.engine.is_empty() {
        return Err("先选一个声音引擎".into());
    }
    let text = st
        .line("voice.test", &[])
        .unwrap_or_else(|| "你好，我是你的桌面伙伴。".into());
    let text = condense(&text, v.max_chars as usize);
    if v.engine == "system" {
        speak(app, v, text);
        return Ok(());
    }
    let id = synth_cached(&st, &v, &text).await?;
    let _ = app.emit_to(
        "pet",
        "voice-play",
        PlayEvent {
            id,
            volume: v.volume as f32 / 100.0,
        },
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn condense_reads_only_the_start() {
        assert_eq!(
            condense("嗯……今天也一起玩吧。我觉得会很开心。", 40),
            "嗯……今天也一起玩吧。我觉得会很开心。"
        );
        assert_eq!(
            condense(
                "（揉揉眼睛）嗯……你来了。我刚刚差点睡着了。要一起聊聊吗？",
                12
            ),
            "嗯……你来了。"
        );
        assert_eq!(condense("好呀😊！来玩井字棋🎮", 40), "好呀！来玩井字棋");
        assert_eq!(
            condense("看这里 https://example.com 很有趣。", 40),
            "看这里很有趣。"
        );
        assert_eq!(
            condense("代码如下：\n```\nfn main() {}\n```\n就这样。", 40),
            "代码如下：就这样。"
        );
        assert_eq!(condense("I like Tetris a lot.", 40), "I like Tetris a lot.");
        // 第一句就很长：在逗号处截断
        let long = "这一关其实不难，只要先把左边的敌人引过来，再绕到后面打它的弱点，就能过去了。";
        let c = condense(long, 20);
        assert!(c.chars().count() <= 20);
        assert!(c.ends_with("过来") || c.ends_with("不难"), "{c}");
        assert_eq!(
            condense(
                "（揉揉眼睛）嗯……你来了。我刚刚差点睡着了。要一起聊聊吗？",
                16
            ),
            "嗯……你来了。我刚刚差点睡着了。"
        );
        assert_eq!(condense("（只有动作）", 40), "");
    }

    #[test]
    fn when_to_speak() {
        let mut v = VoiceSettings {
            enabled: true,
            engine: "system".into(),
            ..Default::default()
        };
        assert!(should_speak(&v, Scene::Chat, false, false));
        assert!(
            should_speak(&v, Scene::Chat, true, false),
            "勿扰时你问她，她照样出声"
        );
        assert!(
            !should_speak(&v, Scene::Proactive, true, false),
            "勿扰时主动说话不出声"
        );
        assert!(
            !should_speak(&v, Scene::Proactive, false, true),
            "全屏时默认不出声"
        );
        v.fullscreen_voice = true;
        assert!(should_speak(&v, Scene::Proactive, false, true));
        assert!(!should_speak(&v, Scene::Reaction, false, true));
        v.scenes.games = false;
        assert!(!should_speak(&v, Scene::Game, false, false));
        v.enabled = false;
        assert!(!should_speak(&v, Scene::Chat, false, false));
    }

    #[test]
    fn sanitize_and_defaults() {
        let d = VoiceSettings::default();
        assert!(!d.enabled, "默认关闭");
        assert_eq!(d.fish_engine, "s2.1-pro-free", "默认用免费的模型");
        let s = VoiceSettings {
            enabled: true,
            engine: "evil".into(),
            fish_url: "http://evil.com".into(),
            fish_model: "abc/../x".into(),
            max_chars: 1,
            speed: f32::NAN,
            ..Default::default()
        }
        .sanitize();
        assert_eq!(s.engine, "");
        assert!(!s.enabled, "没选引擎就不能开");
        assert_eq!(s.fish_url, "https://api.fish.audio", "只能 https 或本机");
        assert_eq!(s.fish_model, "abcx");
        assert_eq!((s.max_chars, s.speed), (10, 1.0));
        assert!(url_ok("http://127.0.0.1:9880") && !url_ok("http://192.168.1.2"));
    }

    #[test]
    fn bodies_and_cache_ids() {
        let mut v = VoiceSettings {
            engine: "fish".into(),
            ..Default::default()
        };
        let b = request_body(&v, "你好");
        assert_eq!(b["reference_id"], FISH_PRESETS[0].0);
        assert_eq!(b["format"], "mp3");
        let id1 = cache_id(&v, "你好");
        assert_eq!(id1.len(), 32);
        assert_ne!(id1, cache_id(&v, "你好呀"));
        v.fish_model = FISH_PRESETS[1].0.into();
        assert_ne!(id1, cache_id(&v, "你好"), "换了声音要重新合成");
        v.engine = "sovits".into();
        v.sovits_ref_audio = "D:/ref.wav".into();
        let b = request_body(&v, "你好");
        assert_eq!(b["text_lang"], "zh");
        assert_eq!(b["ref_audio_path"], "D:/ref.wav");
        v.engine = "openai".into();
        assert_eq!(request_body(&v, "你好")["input"], "你好");
        assert!(!VoiceSettings::default().metered() || VoiceSettings::default().engine.is_empty());
    }

    #[tokio::test]
    async fn fish_request_to_fake_server() {
        let dir = tempfile::tempdir().unwrap();
        let st = crate::state::test_state(dir.path());
        st.secrets.set(FISH_KEY_ID, "fk-test-0000").unwrap();
        let audio = "ID3".to_string() + &"x".repeat(200);
        let (base, rx) =
            crate::backends::http::fake_server::serve(200, "audio/mpeg", vec![audio]).await;
        let v = VoiceSettings {
            enabled: true,
            engine: "fish".into(),
            fish_url: base,
            ..Default::default()
        };
        let id = synth_cached(&st, &v, "你好").await.unwrap();
        let req = rx.await.unwrap();
        let head = req.head.to_ascii_lowercase();
        assert!(head.starts_with("post /v1/tts"));
        assert!(head.contains("authorization: bearer fk-test-0000"));
        assert!(head.contains("model: s2.1-pro-free"));
        assert!(req.body.contains("\"reference_id\""));
        // 第二次直接用缓存（假服务器只回一次，再请求会失败）
        assert_eq!(synth_cached(&st, &v, "你好").await.unwrap(), id);
        assert!(read_cached(&st, &id).unwrap().starts_with(b"ID3"));
        assert!(read_cached(&st, "../../etc/passwd").is_err());
        assert!(clear_cache(&st) > 0);
        assert!(read_cached(&st, &id).is_err());
    }
}
