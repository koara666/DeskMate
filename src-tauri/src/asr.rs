//! 语音输入（默认关闭）：对着麦克风说话 → 本机识别 → 变成一条聊天消息。
//!
//! - 识别全在本机（中文模型：k2-fsa 的 Zipformer-CTC small）：录音不上传、不落盘，说完一句识别完就丢。
//! - 只认中文。挑它而不是 SenseVoice 的依据：在 AISHELL / Common Voice 真人录音上字错率与
//!   SenseVoice 相当（3.3% 对 4.2%、11.4% 对 11.1%），下载只有 SenseVoice 的三分之一；
//!   代价是夹在中文里的英文单词会丢。角色名这类生僻词再用 `asr_names` 按读音纠正。
//! - 模型约 48 MB，安装包没带：第一次打开开关时自动下载（校验 SHA-256），之后离线可用。
//! - 采集用系统默认麦克风（也可以在设置里指定）；Silero VAD 判断一句话说完，说完自动识别。
//! - 点一下开始、再点一下停；也可以设一个全局快捷键。「连续对话」打开时，她说完话会自己接着听。
//! - 她自己在说话时不收音（没有回声消除，免得把她的声音当成用户的话）。
//!
//! 前端通过事件拿到状态（`asr-state`）和识别出来的文字（`asr-text`，只发给聊天窗口），
//! 文字怎么处理（直接发出去 / 先放进输入框）由前端按「识别完自动发送」决定。

use crate::asr_names::NameFixer;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

/// 模型放在数据目录的这个子文件夹里
const MODEL_SUBDIR: &str = "asr-models/zipformer-ctc-zh-small";
/// 第一版用过的多语言模型（SenseVoice，240 MB）；换成中文模型后把它删掉，别白占着磁盘
const OLD_MODEL_SUBDIR: &str = "asr-models/sensevoice";
/// Zipformer-CTC small 中文（sherpa-onnx 官方导出的 int8 版）
const MODEL_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-zipformer-ctc-small-zh-int8-2025-07-16.tar.bz2";
const MODEL_SHA256: &str = "6a71c0cc442ba85ac1455ca23e4561e2a1ef18e55269a184a649d54e9cd0524c";
const MODEL_BYTES: u64 = 50_536_402;
const MODEL_FILE: &str = "model.int8.onnx";
const MODEL_FILE_BYTES: u64 = 62_666_860;
const TOKENS_FILE: &str = "tokens.txt";
/// Silero VAD（断句用）
const VAD_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx";
const VAD_SHA256: &str = "9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6";
const VAD_FILE: &str = "silero_vad.onnx";

/// 点一下开始后，这么久还没人说话就算了（毫秒）
const NO_SPEECH_MS: u128 = 10_000;
/// 连续对话里，这么久没人说话就自己停（毫秒）
const IDLE_CONTINUOUS_MS: u128 = 60_000;
/// 一句话说完后停多久算说完（秒）
const END_SILENCE_S: f32 = 1.0;
/// 断句器是听到声音才开始算的，句首句尾容易被切掉一两个字（实测字错率因此高两个点）：切出来的每一句前后各补回这么多（采样点）
const PAD_SAMPLES: usize = 16_000 * 4 / 10;
/// 留着多长的声音用来补（秒）：比单句最长 25 秒多一点
const BACKLOG_S: usize = 40;
/// 太短的声音（咳嗽、碰到桌子）不识别（采样点，16 kHz）
const MIN_SAMPLES: usize = 16_000 * 3 / 10;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AsrSettings {
    /// 总开关（默认关）
    pub enabled: bool,
    /// 识别完直接发给她（关掉就先放进输入框，自己改完再发）
    pub auto_send: bool,
    /// 连续对话：她说完话自己接着听，不用再点
    pub continuous: bool,
    /// 麦克风（空 = 系统默认）
    pub input_device: String,
    pub hotkey_enabled: bool,
    pub hotkey: String,
}

impl Default for AsrSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_send: true,
            continuous: false,
            input_device: String::new(),
            hotkey_enabled: false,
            hotkey: "Alt+Shift+V".into(),
        }
    }
}

impl AsrSettings {
    pub fn sanitize(mut self) -> Self {
        self.input_device = self.input_device.trim().chars().take(300).collect();
        self.hotkey = self.hotkey.trim().chars().take(60).collect();
        if self.hotkey.is_empty() {
            self.hotkey = Self::default().hotkey;
        }
        self
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Download {
    pub running: bool,
    pub done: u64,
    pub total: u64,
    pub error: String,
}

/// 载入好的识别器（载入要一两秒，所以留着；关掉开关时释放）
struct Engine {
    recognizer: Arc<sherpa_onnx::OfflineRecognizer>,
}

#[derive(Default)]
pub struct AsrState {
    engine: Mutex<Option<Engine>>,
    /// 正在录音的这一轮：停止标志
    session: Mutex<Option<Arc<AtomicBool>>>,
    download: Mutex<Download>,
}

impl AsrState {
    fn engine(&self) -> std::sync::MutexGuard<'_, Option<Engine>> {
        self.engine.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn session(&self) -> std::sync::MutexGuard<'_, Option<Arc<AtomicBool>>> {
        self.session.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn download(&self) -> std::sync::MutexGuard<'_, Download> {
        self.download.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn listening(&self) -> bool {
        self.session().is_some()
    }
}

// ---------------- 模型文件 ----------------

fn model_dir(st: &AppState) -> PathBuf {
    st.paths.root().join(MODEL_SUBDIR)
}

fn file_len(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

/// 三个文件都在、大小对（下载中断留下的半截文件不算）
pub fn model_ready(dir: &Path) -> bool {
    file_len(&dir.join(MODEL_FILE)) == MODEL_FILE_BYTES
        && file_len(&dir.join(TOKENS_FILE)) > 1_000
        && file_len(&dir.join(VAD_FILE)) > 100_000
}

// ---------------- 事件 ----------------

#[derive(Debug, Clone, Serialize)]
struct StateEvent<'a> {
    /// idle / loading / listening / hearing / thinking
    state: &'a str,
    /// 现在的音量（0~1），给界面画电平
    level: f32,
    /// 出错或者要告诉用户的一句话
    note: String,
}

fn emit_state(app: &AppHandle, state: &str, level: f32, note: &str) {
    let _ = app.emit(
        "asr-state",
        StateEvent {
            state,
            level,
            note: note.to_string(),
        },
    );
}

#[derive(Debug, Clone, Serialize)]
struct TextEvent {
    text: String,
    /// 识别完直接发出去，还是先放进输入框
    auto_send: bool,
}

// ---------------- 状态 / 命令 ----------------

#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AsrStatus {
    pub model_ready: bool,
    pub download: Download,
    pub listening: bool,
    pub devices: Vec<DeviceInfo>,
}

fn input_devices() -> Vec<DeviceInfo> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    let Ok(list) = host.input_devices() else {
        return Vec::new();
    };
    list.filter_map(|d| {
        let id = d.id().ok()?.to_string();
        let name = d
            .description()
            .map(|x| x.name().to_string())
            .unwrap_or_else(|_| id.clone());
        Some(DeviceInfo { id, name })
    })
    .collect()
}

pub fn status(st: &AppState) -> AsrStatus {
    AsrStatus {
        model_ready: model_ready(&model_dir(st)),
        download: st.asr.download().clone(),
        listening: st.asr.listening(),
        devices: input_devices(),
    }
}

#[tauri::command]
pub async fn asr_status(app: AppHandle) -> Result<AsrStatus, String> {
    // 枚举设备要和系统音频服务打交道，放到阻塞线程里
    tauri::async_runtime::spawn_blocking(move || status(&app.state::<AppState>()))
        .await
        .map_err(|e| e.to_string())
}

/// 语音输入设置：登记快捷键、保存；打开开关时顺手把模型下好
#[tauri::command]
pub fn asr_save(
    app: AppHandle,
    state: State<'_, AppState>,
    asr: AsrSettings,
) -> Result<AsrSettings, String> {
    let a = asr.sanitize();
    crate::daily::apply_asr_hotkey(&app, a.enabled && a.hotkey_enabled, &a.hotkey)?;
    let saved = state.update_settings(|s| s.asr = a).asr;
    if !saved.enabled {
        stop(&app);
        *state.asr.engine() = None;
    }
    if saved.enabled && !model_ready(&model_dir(&state)) {
        start_download(&app);
    }
    crate::desktop::broadcast_settings_changed(&app);
    Ok(saved)
}

/// 聊天窗口要不要显示麦克风按钮：开关打开、模型已经下好
#[tauri::command]
pub fn asr_available(state: State<'_, AppState>) -> bool {
    state.settings().asr.enabled && model_ready(&model_dir(&state))
}

/// 下载失败后点「重试」
#[tauri::command]
pub fn asr_download(app: AppHandle) {
    start_download(&app);
}

/// 聊天窗口的麦克风按钮：没在听就开始听，在听就停下并识别已经说的
#[tauri::command]
pub fn asr_toggle(app: AppHandle) -> Result<(), String> {
    toggle(&app)
}

pub fn toggle(app: &AppHandle) -> Result<(), String> {
    if app.state::<AppState>().asr.listening() {
        stop(app);
        Ok(())
    } else {
        start(app)
    }
}

/// 快捷键：打开聊天框并开始听 / 停下
pub fn on_hotkey(app: &AppHandle) {
    let st = app.state::<AppState>();
    if st.asr.listening() {
        stop(app);
        return;
    }
    crate::daily::pop_chat(app);
    if let Err(e) = start(app) {
        emit_state(app, "idle", 0.0, &e);
    }
}

/// 开始听
pub fn start(app: &AppHandle) -> Result<(), String> {
    let st = app.state::<AppState>();
    let s = st.settings().asr;
    if !s.enabled {
        return Err("语音输入没有打开：到「设置 → 声音」里打开".into());
    }
    if !model_ready(&model_dir(&st)) {
        start_download(app);
        return Err("语音识别模型还在下载，等下好了再点".into());
    }
    let stop = Arc::new(AtomicBool::new(false));
    {
        let mut g = st.asr.session();
        if g.is_some() {
            return Ok(());
        }
        *g = Some(stop.clone());
    }
    // 用户要说话了：她先闭嘴
    crate::voice::interrupt(app);
    let app2 = app.clone();
    std::thread::Builder::new()
        .name("asr-session".into())
        .spawn(move || {
            let err = run_session(&app2, &stop).err();
            *app2.state::<AppState>().asr.session() = None;
            emit_state(&app2, "idle", 0.0, err.as_deref().unwrap_or(""));
        })
        .map_err(|e| {
            *st.asr.session() = None;
            format!("启动录音失败：{e}")
        })?;
    Ok(())
}

/// 停止（已经说的那一句会先识别完再收尾）
pub fn stop(app: &AppHandle) {
    if let Some(flag) = app.state::<AppState>().asr.session().as_ref() {
        flag.store(true, Ordering::SeqCst);
    }
}

// ---------------- 下载 ----------------

fn start_download(app: &AppHandle) {
    let st = app.state::<AppState>();
    {
        let mut d = st.asr.download();
        if d.running {
            return;
        }
        *d = Download {
            running: true,
            total: MODEL_BYTES,
            ..Download::default()
        };
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = download_model(&app).await;
        let st = app.state::<AppState>();
        {
            let mut d = st.asr.download();
            d.running = false;
            match &result {
                Ok(()) => {
                    d.done = d.total;
                    d.error.clear();
                }
                Err(e) => d.error = e.clone(),
            }
        }
        if let Err(e) = result {
            log::warn!("语音识别模型下载失败：{e}");
        }
        let _ = app.emit("asr-download", st.asr.download().clone());
    });
}

fn set_progress(app: &AppHandle, done: u64) {
    let st = app.state::<AppState>();
    let snapshot = {
        let mut d = st.asr.download();
        d.done = done;
        d.clone()
    };
    let _ = app.emit("asr-download", snapshot);
}

/// 下载一个文件到 `dest`（先写 .part，校验通过再改名），返回前 `on_progress` 会被不断调用
async fn fetch_verified(
    client: &reqwest::Client,
    url: &str,
    sha256: &str,
    dest: &Path,
    mut on_progress: impl FnMut(u64),
) -> Result<(), String> {
    use futures_util::StreamExt;
    let part = dest.with_extension("part");
    let net = |e: reqwest::Error| {
        format!(
            "下载失败（{e}）。检查网络（需要能访问 GitHub，开着代理的话确认它在运行），然后点「重试」"
        )
    };
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(net)?
        .error_for_status()
        .map_err(net)?;
    let mut file = std::fs::File::create(&part).map_err(|e| format!("写文件失败：{e}"))?;
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    let mut last = Instant::now();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(net)?;
        hasher.update(&chunk);
        file.write_all(&chunk)
            .map_err(|e| format!("写文件失败（磁盘满了？）：{e}"))?;
        done += chunk.len() as u64;
        if last.elapsed() > Duration::from_millis(250) {
            on_progress(done);
            last = Instant::now();
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);
    let got: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if got != sha256 {
        let _ = std::fs::remove_file(&part);
        return Err("下载下来的文件校验没通过（可能被网络环境改动了），点「重试」".into());
    }
    std::fs::rename(&part, dest).map_err(|e| format!("保存文件失败：{e}"))
}

/// 从下载好的 tar.bz2 里取出模型和词表
fn extract_model(archive: &Path, dir: &Path) -> Result<(), String> {
    let f = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut tar = tar::Archive::new(bzip2::read::BzDecoder::new(f));
    let mut got = 0;
    for entry in tar.entries().map_err(|e| format!("解压失败：{e}"))? {
        let mut entry = entry.map_err(|e| format!("解压失败：{e}"))?;
        let name = entry
            .path()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_default();
        if name == MODEL_FILE || name == TOKENS_FILE {
            let part = dir.join(format!("{name}.part"));
            entry
                .unpack(&part)
                .map_err(|e| format!("解压失败（磁盘满了？）：{e}"))?;
            std::fs::rename(&part, dir.join(&name)).map_err(|e| e.to_string())?;
            got += 1;
        }
    }
    if got == 2 {
        Ok(())
    } else {
        Err("模型压缩包里缺文件，点「重试」重新下载".into())
    }
}

async fn download_model(app: &AppHandle) -> Result<(), String> {
    let st = app.state::<AppState>();
    let dir = model_dir(&st);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建文件夹失败：{e}"))?;
    let client = st.http.clone();
    if file_len(&dir.join(VAD_FILE)) < 100_000 {
        fetch_verified(&client, VAD_URL, VAD_SHA256, &dir.join(VAD_FILE), |_| {}).await?;
    }
    if file_len(&dir.join(MODEL_FILE)) != MODEL_FILE_BYTES
        || file_len(&dir.join(TOKENS_FILE)) < 1_000
    {
        let archive = dir.join("model.tar.bz2");
        let app2 = app.clone();
        fetch_verified(&client, MODEL_URL, MODEL_SHA256, &archive, move |done| {
            set_progress(&app2, done)
        })
        .await?;
        let (a, d) = (archive.clone(), dir.clone());
        tauri::async_runtime::spawn_blocking(move || extract_model(&a, &d))
            .await
            .map_err(|e| e.to_string())??;
        let _ = std::fs::remove_file(&archive);
    }
    if model_ready(&dir) {
        remove_old_model(&st);
        Ok(())
    } else {
        Err("模型文件不完整，点「重试」".into())
    }
}

/// 新模型就绪后，把第一版的多语言模型删掉（240 MB）；新模型没好之前不动它
fn remove_old_model(st: &AppState) {
    if !model_ready(&model_dir(st)) {
        return;
    }
    let old = st.paths.root().join(OLD_MODEL_SUBDIR);
    if old.exists() {
        match std::fs::remove_dir_all(&old) {
            Ok(()) => log::info!("已删除旧的语音识别模型"),
            Err(e) => log::warn!("删除旧的语音识别模型失败：{e}"),
        }
    }
}

/// 启动时：语音输入是开着的、模型却没下好（比如从旧版本升级，模型换了）就接着下；新模型就绪后清掉旧的
pub fn on_startup(app: &AppHandle) {
    let st = app.state::<AppState>();
    if st.settings().asr.enabled && !model_ready(&model_dir(&st)) {
        start_download(app);
    }
    remove_old_model(&st);
}

// ---------------- 识别 ----------------

/// 识别器配置：模型文件夹里的 Zipformer-CTC 中文模型
fn recognizer_config(dir: &Path) -> sherpa_onnx::OfflineRecognizerConfig {
    let mut cfg = sherpa_onnx::OfflineRecognizerConfig::default();
    cfg.model_config.zipformer_ctc = sherpa_onnx::OfflineZipformerCtcModelConfig {
        model: Some(dir.join(MODEL_FILE).to_string_lossy().to_string()),
    };
    cfg.model_config.tokens = Some(dir.join(TOKENS_FILE).to_string_lossy().to_string());
    cfg.model_config.num_threads = 2;
    cfg.model_config.provider = Some("cpu".into());
    cfg
}

fn load_engine(app: &AppHandle) -> Result<Arc<sherpa_onnx::OfflineRecognizer>, String> {
    let st = app.state::<AppState>();
    let mut g = st.asr.engine();
    if let Some(e) = g.as_ref() {
        return Ok(e.recognizer.clone());
    }
    let cfg = recognizer_config(&model_dir(&st));
    let rec = sherpa_onnx::OfflineRecognizer::create(&cfg)
        .ok_or("载入语音识别模型失败：文件可能损坏了，到设置里点「重试」重新下载")?;
    let rec = Arc::new(rec);
    *g = Some(Engine {
        recognizer: rec.clone(),
    });
    Ok(rec)
}

/// 当前角色的名字和叫法（全名、简称、外号），用来纠正识别结果里听错的称呼
fn name_fixer(app: &AppHandle) -> NameFixer {
    let Some(c) = app.state::<AppState>().current_character() else {
        return NameFixer::default();
    };
    NameFixer::new(std::iter::once(c.manifest.name).chain(c.manifest.aliases))
}

/// 去掉模型偶尔带上的 `<|zh|>` 这类标签；只剩标点的也算没听到
pub fn clean_text(raw: &str) -> String {
    let mut out = String::new();
    let mut rest = raw;
    while let Some(i) = rest.find("<|") {
        out.push_str(&rest[..i]);
        match rest[i..].find("|>") {
            Some(j) => rest = &rest[i + j + 2..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    let t = out.trim().to_string();
    if t.chars().any(|c| c.is_alphanumeric()) {
        t
    } else {
        String::new()
    }
}

/// 断句器配置：说完停 0.8 秒算一句，单句最长 25 秒
fn vad_config(dir: &Path) -> sherpa_onnx::VadModelConfig {
    sherpa_onnx::VadModelConfig {
        silero_vad: sherpa_onnx::SileroVadModelConfig {
            model: Some(dir.join(VAD_FILE).to_string_lossy().to_string()),
            threshold: 0.5,
            min_silence_duration: END_SILENCE_S,
            min_speech_duration: 0.25,
            window_size: 512,
            max_speech_duration: 25.0,
        },
        sample_rate: 16_000,
        num_threads: 1,
        provider: Some("cpu".into()),
        debug: false,
        ..Default::default()
    }
}

/// 最近一段已经交给断句器的声音，切出一句后按位置把前后各补一点
#[derive(Default)]
struct Backlog {
    buf: std::collections::VecDeque<f32>,
    /// `buf[0]` 在整条声音里的位置
    base: u64,
    /// 已经交给断句器的总采样数
    fed: u64,
    /// 断句器上次重置时的位置（句子的 `start` 是相对它的）
    origin: u64,
}

impl Backlog {
    fn push(&mut self, samples: &[f32]) {
        self.buf.extend(samples);
        self.fed += samples.len() as u64;
        let max = 16_000 * BACKLOG_S;
        if self.buf.len() > max {
            let drop = self.buf.len() - max;
            self.buf.drain(..drop);
            self.base += drop as u64;
        }
    }

    /// 断句器重置了：之前的声音不再参与补音
    fn reset(&mut self) {
        self.buf.clear();
        self.base = self.fed;
        self.origin = self.fed;
    }

    /// 断句器切出的一句（`start` 起、长 `len`）前后各补 `PAD_SAMPLES`，补不到的地方（刚开始 / 刚停）就少补
    fn padded(&self, start: i64, len: usize) -> Vec<f32> {
        let from = (self.origin + start.max(0) as u64)
            .saturating_sub(PAD_SAMPLES as u64)
            .max(self.base);
        let to =
            (self.origin + start.max(0) as u64 + len as u64 + PAD_SAMPLES as u64).min(self.fed);
        if to <= from {
            return Vec::new();
        }
        self.buf
            .range((from - self.base) as usize..(to - self.base) as usize)
            .copied()
            .collect()
    }
}

fn recognize(rec: &sherpa_onnx::OfflineRecognizer, samples: &[f32]) -> String {
    let stream = rec.create_stream();
    stream.accept_waveform(16_000, samples);
    rec.decode(&stream);
    stream
        .get_result()
        .map(|r| clean_text(&r.text))
        .unwrap_or_default()
}

// ---------------- 采集 ----------------

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|x| x * x).sum::<f32>() / samples.len() as f32).sqrt()
}

/// 打开麦克风，把采到的声音（单声道 f32，原采样率）送进 `tx`。返回的流要一直拿着，丢掉就停了。
fn open_capture(
    device_id: &str,
    tx: std::sync::mpsc::Sender<Vec<f32>>,
) -> Result<(cpal::Stream, u32), String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let host = cpal::default_host();
    let device = if device_id.is_empty() {
        host.default_input_device()
    } else {
        device_id
            .parse::<cpal::DeviceId>()
            .ok()
            .and_then(|id| host.device_by_id(&id))
            .or_else(|| host.default_input_device())
    }
    .ok_or("没有找到麦克风：确认电脑接了麦克风，并且在系统设置里允许桌面应用使用麦克风")?;
    let config = device.default_input_config().map_err(|e| {
        format!("打不开麦克风（{e}）：在系统设置 → 隐私 → 麦克风里允许桌面应用使用")
    })?;
    let rate = config.sample_rate();
    let channels = config.channels() as usize;
    let format = config.sample_format();
    let sc: cpal::StreamConfig = config.into();
    let err_fn = |e: cpal::Error| log::warn!("麦克风出错：{e}");
    macro_rules! build {
        ($t:ty) => {{
            let tx = tx.clone();
            device.build_input_stream(
                sc.clone(),
                move |data: &[$t], _: &cpal::InputCallbackInfo| {
                    let mono: Vec<f32> = data
                        .chunks(channels.max(1))
                        .map(|f| {
                            f.iter()
                                .map(|&x| <f32 as cpal::FromSample<$t>>::from_sample_(x))
                                .sum::<f32>()
                                / f.len() as f32
                        })
                        .collect();
                    let _ = tx.send(mono);
                },
                err_fn,
                None,
            )
        }};
    }
    let stream = match format {
        cpal::SampleFormat::F32 => build!(f32),
        cpal::SampleFormat::I16 => build!(i16),
        cpal::SampleFormat::I32 => build!(i32),
        cpal::SampleFormat::U16 => build!(u16),
        other => return Err(format!("不支持这个麦克风的音频格式（{other}）")),
    }
    .map_err(|e| format!("打不开麦克风（{e}）：在系统设置 → 隐私 → 麦克风里允许桌面应用使用"))?;
    stream
        .play()
        .map_err(|e| format!("麦克风没法开始录音（{e}）"))?;
    Ok((stream, rate))
}

/// 录一轮：一直听，说完一句就识别；一次性模式识别出内容就结束，连续对话模式接着听
fn run_session(app: &AppHandle, stop: &Arc<AtomicBool>) -> Result<(), String> {
    let st = app.state::<AppState>();
    let s = st.settings().asr;
    emit_state(app, "loading", 0.0, "");
    let recognizer = load_engine(app)?;
    let names = name_fixer(app);
    let dir = model_dir(&st);

    let vad_cfg = vad_config(&dir);
    let vad = sherpa_onnx::VoiceActivityDetector::create(&vad_cfg, 30.0)
        .ok_or("载入断句模型失败：到设置里点「重试」重新下载")?;

    let (tx, rx) = std::sync::mpsc::channel::<Vec<f32>>();
    let (_stream, rate) = open_capture(&s.input_device, tx)?;
    let resampler =
        sherpa_onnx::LinearResampler::create(rate as i32, 16_000).ok_or("音频重采样初始化失败")?;

    emit_state(app, "listening", 0.0, "");
    let started = Instant::now();
    let mut last_speech = Instant::now();
    let mut last_emit = Instant::now();
    let mut hearing = false;
    let mut spoke_once = false;
    let mut stopping = false;
    let mut backlog = Backlog::default();

    loop {
        if stop.load(Ordering::SeqCst) && !stopping {
            // 把还没断句的那一句也识别掉
            stopping = true;
            vad.flush();
        }
        if !stopping {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(chunk) => {
                    // 她正在说话：不收音，也别让没说完的半句留在断句器里
                    if crate::voice::is_speaking(app) {
                        vad.reset();
                        backlog.reset();
                        hearing = false;
                        last_speech = Instant::now();
                        continue;
                    }
                    let samples = resampler.resample(&chunk, false);
                    if samples.is_empty() {
                        continue;
                    }
                    backlog.push(&samples);
                    vad.accept_waveform(&samples);
                    let detected = vad.detected();
                    if detected {
                        last_speech = Instant::now();
                    }
                    if last_emit.elapsed() > Duration::from_millis(100) || detected != hearing {
                        hearing = detected;
                        let level = (rms(&samples) * 8.0).min(1.0);
                        emit_state(
                            app,
                            if hearing { "hearing" } else { "listening" },
                            level,
                            "",
                        );
                        last_emit = Instant::now();
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    return Err("麦克风断开了（拔掉或者被别的程序占用了）".into());
                }
            }
        }
        // 说完的句子
        let mut finished = false;
        while !vad.is_empty() {
            let Some(seg) = vad.front() else { break };
            let (start, len) = (seg.start(), seg.samples().len());
            let mut samples = backlog.padded(start, len);
            if samples.is_empty() {
                samples = seg.samples().to_vec();
            }
            vad.pop();
            if len < MIN_SAMPLES {
                continue;
            }
            hearing = false;
            emit_state(app, "thinking", 0.0, "");
            let text = names.fix(&recognize(&recognizer, &samples));
            if !text.is_empty() {
                spoke_once = true;
                let _ = app.emit_to(
                    "chat",
                    "asr-text",
                    TextEvent {
                        text,
                        auto_send: s.auto_send,
                    },
                );
                if !s.continuous {
                    finished = true;
                    break;
                }
            }
            last_speech = Instant::now();
            if !stopping {
                emit_state(app, "listening", 0.0, "");
            }
        }
        if finished || (stopping && vad.is_empty()) {
            return Ok(());
        }
        // 没人说话就别一直开着麦
        let idle = last_speech.elapsed().as_millis();
        if !hearing && !spoke_once && started.elapsed().as_millis() > NO_SPEECH_MS && !s.continuous
        {
            return Err("没听到你说话".into());
        }
        if !hearing && s.continuous && idle > IDLE_CONTINUOUS_MS {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_text_drops_tags_and_empty_results() {
        assert_eq!(clean_text("<|zh|><|NEUTRAL|>你好呀"), "你好呀");
        assert_eq!(clean_text("  今天天气不错。 "), "今天天气不错。");
        assert_eq!(clean_text("。。"), "");
        assert_eq!(clean_text("<|en|>"), "");
        assert_eq!(clean_text("<|zh|"), "");
    }

    /// 这台电脑有麦克风、系统也允许桌面应用使用时，能打开并持续收到声音（设了 `DESKMATE_MIC_TEST=1` 才跑）
    #[test]
    fn default_microphone_delivers_samples() {
        if std::env::var("DESKMATE_MIC_TEST").is_err() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let (_stream, rate) = open_capture("", tx).expect("打开麦克风");
        assert!(rate >= 8_000, "{rate}");
        let (mut n, mut peak) = (0usize, 0f32);
        let t0 = Instant::now();
        while t0.elapsed() < Duration::from_millis(1500) {
            if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(200)) {
                n += chunk.len();
                peak = chunk.iter().fold(peak, |p, x| p.max(x.abs()));
            }
        }
        println!("1.5 秒收到 {n} 个采样点（{rate} Hz），峰值 {peak:.4}");
        assert!(n as f32 > rate as f32, "收到的采样点太少：{n}");
    }

    #[test]
    fn settings_are_sanitized() {
        let s = AsrSettings {
            hotkey: "  ".into(),
            input_device: "  mic ".into(),
            ..AsrSettings::default()
        }
        .sanitize();
        assert_eq!(s.hotkey, "Alt+Shift+V");
        assert_eq!(s.input_device, "mic");
        assert!(!s.enabled, "默认关闭");
        assert!(s.auto_send);
    }

    #[test]
    fn backlog_pads_both_sides_and_clamps_at_the_edges() {
        let mut b = Backlog::default();
        // 每个采样点的值就是它的位置，方便对
        let audio: Vec<f32> = (0..16_000 * 5).map(|i| i as f32).collect();
        for c in audio.chunks(1600) {
            b.push(c);
        }
        let pad = PAD_SAMPLES;
        // 句子在 2 秒处，长 1 秒：前后各补
        let seg = b.padded(32_000, 16_000);
        assert_eq!(seg.len(), 16_000 + 2 * pad);
        assert_eq!(seg[0], (32_000 - pad) as f32);
        // 贴着开头：前面补不到，只补到开头
        let seg = b.padded(1_000, 8_000);
        assert_eq!((seg[0], seg.len()), (0.0, 1_000 + 8_000 + pad));
        // 贴着末尾：后面补不到
        let seg = b.padded(80_000 - 8_000, 8_000);
        assert_eq!(seg.len(), pad + 8_000);
        // 断句器重置后，位置从新的起点算，之前的声音不参与
        b.reset();
        b.push(&[1.0; 100]);
        assert_eq!(b.padded(0, 100), vec![1.0; 100]);
        // 保留时长之外的老声音被丢掉，位置仍然对得上
        let mut b = Backlog::default();
        let long: Vec<f32> = (0..16_000 * (BACKLOG_S + 10)).map(|i| i as f32).collect();
        for c in long.chunks(16_000) {
            b.push(c);
        }
        assert!(b.buf.len() <= 16_000 * BACKLOG_S);
        let at = 16_000 * (BACKLOG_S + 5);
        assert_eq!(b.padded(at as i64, 1_000)[pad], at as f32);
        // 太老的位置已经没有了：不 panic，给空
        assert!(b.padded(0, 100).is_empty());
    }

    /// 造一个只含需要的两个文件（外加一个不需要的）的 tar.bz2
    fn fake_archive(path: &Path, with_tokens: bool) {
        let enc = bzip2::write::BzEncoder::new(
            std::fs::File::create(path).unwrap(),
            bzip2::Compression::fast(),
        );
        let mut tar = tar::Builder::new(enc);
        let mut add = |name: &str, data: &[u8]| {
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            tar.append_data(&mut h, format!("model-dir/{name}"), data)
                .unwrap();
        };
        add(MODEL_FILE, b"onnx");
        add("README.md", b"not needed");
        if with_tokens {
            add(TOKENS_FILE, b"<blk> 0");
        }
        tar.into_inner().unwrap().finish().unwrap();
    }

    #[test]
    fn extract_takes_only_the_two_files_and_complains_about_missing_ones() {
        let d = tempfile::tempdir().unwrap();
        let (ok, bad, out) = (
            d.path().join("a.tar.bz2"),
            d.path().join("b.tar.bz2"),
            d.path().join("out"),
        );
        std::fs::create_dir(&out).unwrap();
        fake_archive(&ok, true);
        extract_model(&ok, &out).unwrap();
        assert_eq!(std::fs::read(out.join(MODEL_FILE)).unwrap(), b"onnx");
        assert!(out.join(TOKENS_FILE).exists());
        assert!(!out.join("README.md").exists());
        assert!(
            std::fs::read_dir(&out).unwrap().all(|e| !e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".part"))
        );
        fake_archive(&bad, false);
        assert!(extract_model(&bad, &out).is_err(), "缺词表要报错");
    }

    /// 官方压缩包能解出可用的模型文件（`DESKMATE_ASR_ARCHIVE` 指向下载好的 tar.bz2；校验值也顺带核对）
    #[test]
    fn real_archive_matches_the_pinned_checksum_and_extracts() {
        let Ok(archive) = std::env::var("DESKMATE_ASR_ARCHIVE") else {
            return;
        };
        let archive = PathBuf::from(archive);
        let bytes = std::fs::read(&archive).unwrap();
        assert_eq!(bytes.len() as u64, MODEL_BYTES);
        let got: String = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(got, MODEL_SHA256);
        let d = tempfile::tempdir().unwrap();
        extract_model(&archive, d.path()).unwrap();
        assert_eq!(file_len(&d.path().join(MODEL_FILE)), MODEL_FILE_BYTES);
        assert!(file_len(&d.path().join(TOKENS_FILE)) > 1_000);
        // 解出来的文件能真的载入成识别器
        assert!(sherpa_onnx::OfflineRecognizer::create(&recognizer_config(d.path())).is_some());
    }

    #[test]
    fn model_needs_all_files_with_the_right_size() {
        let d = tempfile::tempdir().unwrap();
        assert!(!model_ready(d.path()));
        std::fs::write(d.path().join(TOKENS_FILE), vec![b'a'; 2_000]).unwrap();
        std::fs::write(d.path().join(VAD_FILE), vec![0u8; 200_000]).unwrap();
        // 模型文件只有半截：不算好
        std::fs::write(d.path().join(MODEL_FILE), vec![0u8; 1000]).unwrap();
        assert!(!model_ready(d.path()));
    }

    /// 一段录音按真实流程走一遍：前后垫静音 → 断句器切句 → 每句识别 → 拼起来 → 名字纠正
    fn run_pipeline(
        rec: &sherpa_onnx::OfflineRecognizer,
        vad: &sherpa_onnx::VoiceActivityDetector,
        names: &NameFixer,
        wav: &[f32],
    ) -> String {
        vad.reset();
        let mut backlog = Backlog::default();
        let mut samples = vec![0.0f32; 16_000];
        samples.extend_from_slice(wav);
        samples.extend(vec![0.0f32; 16_000 * 2]);
        for chunk in samples.chunks(1600) {
            backlog.push(chunk);
            vad.accept_waveform(chunk);
        }
        vad.flush();
        let mut text = String::new();
        while let Some(seg) = vad.front() {
            let (start, len) = (seg.start(), seg.samples().len());
            let padded = backlog.padded(start, len);
            vad.pop();
            if len >= MIN_SAMPLES {
                text.push_str(&recognize(rec, &padded));
            }
        }
        names.fix(&text)
    }

    /// 只留汉字、字母、数字，字母小写
    fn norm(s: &str) -> Vec<char> {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(|c| c.to_lowercase())
            .collect()
    }

    fn edit_distance(a: &[char], b: &[char]) -> usize {
        let mut prev: Vec<usize> = (0..=b.len()).collect();
        for (i, ca) in a.iter().enumerate() {
            let mut cur = vec![i + 1];
            for (j, cb) in b.iter().enumerate() {
                cur.push(
                    (prev[j + 1] + 1)
                        .min(cur[j] + 1)
                        .min(prev[j] + usize::from(ca != cb)),
                );
            }
            prev = cur;
        }
        prev[b.len()]
    }

    /// 本机有模型时，用真人录音走一遍整条识别流程（`DESKMATE_ASR_TEST_DIR` 指向放了
    /// 模型、tokens.txt、silero_vad.onnx 的文件夹）。
    /// 另外给 `DESKMATE_ASR_EVAL_TSV`（每行 `wav 路径<TAB>正确文字<TAB>时长`）就统计字错率，
    /// `DESKMATE_ASR_EVAL_MAX` 限制条数（默认 80），`DESKMATE_ASR_EVAL_NAMES` 给名字纠正用（逗号分隔），
    /// `DESKMATE_ASR_EVAL_MAX_CER` 是允许的最大字错率（百分数，默认 15）。
    #[test]
    fn pipeline_recognizes_real_recordings_when_the_model_is_present() {
        let Ok(dir) = std::env::var("DESKMATE_ASR_TEST_DIR") else {
            return;
        };
        let dir = PathBuf::from(dir);
        let rec =
            sherpa_onnx::OfflineRecognizer::create(&recognizer_config(&dir)).expect("载入模型");
        let vad =
            sherpa_onnx::VoiceActivityDetector::create(&vad_config(&dir), 30.0).expect("断句模型");
        let names = NameFixer::new(
            std::env::var("DESKMATE_ASR_EVAL_NAMES")
                .unwrap_or_default()
                .split(',')
                .map(str::to_string),
        );

        let Ok(tsv) = std::env::var("DESKMATE_ASR_EVAL_TSV") else {
            // 没给数据集：模型自带的两段中文录音至少要能识别出东西
            for wav in ["0.wav", "1.wav"] {
                let w = sherpa_onnx::Wave::read(&dir.join("test_wavs").join(wav).to_string_lossy())
                    .expect("读 wav");
                let text = run_pipeline(&rec, &vad, &names, w.samples());
                println!("{wav} → {text}");
                assert!(!text.is_empty(), "{wav} 没识别出东西");
            }
            return;
        };
        let max: usize = std::env::var("DESKMATE_ASR_EVAL_MAX")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(80);
        let limit: f64 = std::env::var("DESKMATE_ASR_EVAL_MAX_CER")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(15.0);
        let base = Path::new(&tsv)
            .parent()
            .and_then(Path::parent)
            .expect("数据集目录");
        let (mut errs, mut total, mut empty, mut n) = (0usize, 0usize, 0usize, 0usize);
        let mut direct_errs = 0usize;
        let lines = std::fs::read_to_string(&tsv).expect("读 tsv");
        let all: Vec<&str> = lines.lines().filter(|l| !l.trim().is_empty()).collect();
        // 均匀抽样，别只取开头
        let step = (all.len() / max.max(1)).max(1);
        for line in all.iter().step_by(step).take(max) {
            let mut cols = line.split('\t');
            let (path, reference) = (cols.next().unwrap(), cols.next().unwrap_or(""));
            let w = sherpa_onnx::Wave::read(&base.join(path).to_string_lossy()).expect("读 wav");
            let got = run_pipeline(&rec, &vad, &names, w.samples());
            let (r, h) = (norm(reference), norm(&got));
            // 对照：不经断句，整段直接识别
            direct_errs += edit_distance(&r, &norm(&names.fix(&recognize(&rec, w.samples()))));
            if h.is_empty() {
                empty += 1;
            }
            errs += edit_distance(&r, &h);
            total += r.len();
            n += 1;
        }
        let cer = errs as f64 / total as f64 * 100.0;
        let direct = direct_errs as f64 / total as f64 * 100.0;
        println!(
            "整条流程：{n} 条录音，字错率 {cer:.2}%（不经断句直接识别 {direct:.2}%），没识别出东西的 {empty} 条"
        );
        assert!(cer < limit, "字错率 {cer:.2}% 超过了 {limit}%");
        assert!(empty * 20 <= n, "断句漏掉的太多：{empty}/{n}");
    }
}
