//! 天气：Open-Meteo（免费、不需要 Key，非商用、需署名 CC BY 4.0；2026-10-28 查阅
//! https://open-meteo.com/en/docs 和 https://open-meteo.com/en/terms）。
//! 只发坐标，返回纯数字的结构化数据；失败时安静跳过（播报里不提天气）。同一城市一小时内用缓存。

use super::city::City;
use serde::Deserialize;
use std::sync::Mutex;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct Day {
    pub code: i32,
    pub max: f64,
    pub min: f64,
    /// 最大降水概率（%）
    pub rain_prob: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Weather {
    pub city: String,
    pub now_temp: Option<f64>,
    pub now_code: Option<i32>,
    /// 今天、明天、后天
    pub days: Vec<Day>,
}

/// WMO 天气代码 → 中文
pub fn describe(code: i32) -> &'static str {
    match code {
        0 => "晴",
        1 => "晴间多云",
        2 => "多云",
        3 => "阴",
        45 | 48 => "有雾",
        51 | 53 | 55 => "毛毛雨",
        56 | 57 | 66 | 67 => "冻雨",
        61 => "小雨",
        63 => "中雨",
        65 => "大雨",
        71 => "小雪",
        73 => "中雪",
        75 | 77 => "大雪",
        80 | 81 => "阵雨",
        82 => "强阵雨",
        85 | 86 => "阵雪",
        95 => "雷阵雨",
        96 | 97 | 99 => "雷暴",
        _ => "天气不明",
    }
}

pub fn is_rain(code: i32) -> bool {
    matches!(code, 51..=67 | 80..=82 | 95..=99)
}

pub fn is_snow(code: i32) -> bool {
    matches!(code, 71..=77 | 85 | 86)
}

/// 天气对出门的影响（台词池场景名）：带伞、加衣、防暑、适合出门……
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Advice {
    Umbrella,
    Snow,
    Cold,
    Hot,
    TempSwing,
    Nice,
}

impl Advice {
    pub fn scene(self) -> &'static str {
        match self {
            Advice::Umbrella => "briefing.umbrella",
            Advice::Snow => "briefing.snow",
            Advice::Cold => "briefing.cold",
            Advice::Hot => "briefing.hot",
            Advice::TempSwing => "briefing.temp_swing",
            Advice::Nice => "briefing.nice",
        }
    }
    /// 给模型的一句提示
    pub fn hint(self) -> &'static str {
        match self {
            Advice::Umbrella => "可能下雨，出门带伞",
            Advice::Snow => "下雪，路滑、注意保暖",
            Advice::Cold => "很冷，多穿点",
            Advice::Hot => "很热，注意防暑、多喝水",
            Advice::TempSwing => "早晚温差大，带件外套",
            Advice::Nice => "天气不错，适合出门",
        }
    }
}

/// 本地规则给出建议（最多两条，重要的在前）
pub fn advice(d: &Day) -> Vec<Advice> {
    let mut v = Vec::new();
    if is_snow(d.code) {
        v.push(Advice::Snow);
    } else if is_rain(d.code) || d.rain_prob.is_some_and(|p| p >= 50.0) {
        v.push(Advice::Umbrella);
    }
    if d.max >= 32.0 {
        v.push(Advice::Hot);
    } else if d.min <= 5.0 {
        v.push(Advice::Cold);
    } else if d.max - d.min >= 10.0 {
        v.push(Advice::TempSwing);
    }
    if v.is_empty() && d.code <= 2 && (15.0..=28.0).contains(&d.max) {
        v.push(Advice::Nice);
    }
    v.truncate(2);
    v
}

fn day_text(label: &str, d: &Day) -> String {
    let mut t = format!(
        "{label}{}，{}~{}°C",
        describe(d.code),
        d.min.round() as i64,
        d.max.round() as i64
    );
    if let Some(p) = d.rain_prob
        && p >= 20.0
    {
        t.push_str(&format!("，降水概率 {}%", p.round() as i64));
    }
    t
}

impl Weather {
    /// 一句话描述（今天；`with_tomorrow` 时加上明天）
    pub fn summary(&self, with_tomorrow: bool) -> String {
        let mut parts = Vec::new();
        if let Some(d) = self.days.first() {
            parts.push(day_text("今天", d));
        }
        if with_tomorrow && let Some(d) = self.days.get(1) {
            parts.push(day_text("明天", d));
        }
        let mut s = format!("{}：{}", self.city, parts.join("；"));
        if let (Some(t), Some(c)) = (self.now_temp, self.now_code) {
            s.push_str(&format!("（现在{}，{}°C）", describe(c), t.round() as i64));
        }
        s
    }

    /// 给模型的完整说明（三天 + 建议）
    pub fn for_prompt(&self) -> String {
        let mut s = self.summary(true);
        if let Some(d) = self.days.get(2) {
            s.push_str(&format!("；{}", day_text("后天", d)));
        }
        if let Some(d) = self.days.first() {
            let a: Vec<&str> = advice(d).iter().map(|x| x.hint()).collect();
            if !a.is_empty() {
                s.push_str(&format!("。今天的建议：{}", a.join("；")));
            }
        }
        s
    }
}

#[derive(Deserialize)]
struct Resp {
    #[serde(default)]
    current: Option<Current>,
    daily: Daily,
}

#[derive(Deserialize)]
struct Current {
    temperature_2m: Option<f64>,
    weather_code: Option<i32>,
}

#[derive(Deserialize)]
struct Daily {
    #[serde(default)]
    weather_code: Vec<Option<i32>>,
    #[serde(default)]
    temperature_2m_max: Vec<Option<f64>>,
    #[serde(default)]
    temperature_2m_min: Vec<Option<f64>>,
    #[serde(default)]
    precipitation_probability_max: Vec<Option<f64>>,
}

/// 解析接口返回（纯函数，便于测试）
pub fn parse(city: &str, body: &str) -> Option<Weather> {
    let r: Resp = serde_json::from_str(body).ok()?;
    let n = r.daily.weather_code.len().min(3);
    let mut days = Vec::new();
    for i in 0..n {
        days.push(Day {
            code: r.daily.weather_code[i]?,
            max: (*r.daily.temperature_2m_max.get(i)?)?,
            min: (*r.daily.temperature_2m_min.get(i)?)?,
            rain_prob: r
                .daily
                .precipitation_probability_max
                .get(i)
                .copied()
                .flatten(),
        });
    }
    if days.is_empty() {
        return None;
    }
    Some(Weather {
        city: city.to_string(),
        now_temp: r.current.as_ref().and_then(|c| c.temperature_2m),
        now_code: r.current.as_ref().and_then(|c| c.weather_code),
        days,
    })
}

pub const API: &str = "https://api.open-meteo.com/v1/forecast";

/// 查天气（10 秒超时）。`base` 一般是 `API`，测试时换成本地假服务器。
pub async fn fetch(http: &reqwest::Client, base: &str, city: &City) -> Option<Weather> {
    let lat = format!("{:.4}", city.lat);
    let lon = format!("{:.4}", city.lon);
    let url = reqwest::Url::parse_with_params(
        base,
        &[
            ("latitude", lat.as_str()),
            ("longitude", lon.as_str()),
            ("current", "temperature_2m,weather_code"),
            (
                "daily",
                "weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max",
            ),
            ("timezone", "auto"),
            ("forecast_days", "3"),
        ],
    )
    .ok()?;
    let resp = http
        .get(url)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| {
            log::info!(
                "天气查询失败：{}",
                if e.is_timeout() { "超时" } else { "网络" }
            )
        })
        .ok()?;
    if !resp.status().is_success() {
        log::info!("天气查询失败：{}", resp.status().as_u16());
        return None;
    }
    let body = resp.text().await.ok()?;
    if body.len() > 200_000 {
        return None;
    }
    parse(&city.name, &body)
}

/// 一小时缓存（同一个城市）
#[derive(Default)]
pub struct Cache {
    inner: Mutex<Vec<(String, i64, Weather)>>,
}

const CACHE_SECS: i64 = 3600;

impl Cache {
    pub fn get(&self, key: &str, now: i64) -> Option<Weather> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|(k, t, _)| k == key && now - t < CACHE_SECS)
            .map(|(_, _, w)| w.clone())
    }
    pub fn put(&self, key: &str, now: i64, w: Weather) {
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        g.retain(|(k, t, _)| k != key && now - t < CACHE_SECS);
        g.push((key.to_string(), now, w));
        if g.len() > 8 {
            g.remove(0);
        }
    }
}

pub fn cache_key(c: &City) -> String {
    format!("{:.2},{:.2}", c.lat, c.lon)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"latitude":34.69,"longitude":135.5,"timezone":"Asia/Tokyo",
      "current":{"time":"2026-10-28T15:00","temperature_2m":17.4,"weather_code":61},
      "daily":{"time":["2026-10-28","2026-10-29","2026-10-30"],
        "weather_code":[61,1,3],"temperature_2m_max":[19.5,21.0,18.2],"temperature_2m_min":[8.7,11.0,12.0],
        "precipitation_probability_max":[70,5,30]}}"#;

    #[test]
    fn parses_and_describes() {
        let w = parse("大阪", SAMPLE).unwrap();
        assert_eq!(w.days.len(), 3);
        assert_eq!(w.days[0].code, 61);
        assert_eq!(
            w.summary(true),
            "大阪：今天小雨，9~20°C，降水概率 70%；明天晴间多云，11~21°C（现在小雨，17°C）"
        );
        let p = w.for_prompt();
        assert!(p.contains("后天阴") && p.contains("带伞") && p.contains("温差"));
        assert!(parse("x", "{}").is_none());
        assert!(parse("x", "not json").is_none());
    }

    #[test]
    fn advice_rules() {
        let d = |code, max, min, p: Option<f64>| Day {
            code,
            max,
            min,
            rain_prob: p,
        };
        assert_eq!(advice(&d(0, 22.0, 14.0, Some(0.0))), vec![Advice::Nice]);
        assert_eq!(
            advice(&d(3, 20.0, 15.0, Some(60.0))),
            vec![Advice::Umbrella]
        );
        assert_eq!(advice(&d(0, 35.0, 26.0, None)), vec![Advice::Hot]);
        assert_eq!(
            advice(&d(71, 1.0, -4.0, None)),
            vec![Advice::Snow, Advice::Cold]
        );
        assert_eq!(advice(&d(2, 24.0, 10.0, None)), vec![Advice::TempSwing]);
        assert_eq!(advice(&d(3, 20.0, 15.0, Some(10.0))), vec![]);
    }

    #[test]
    fn cache_expires() {
        let c = Cache::default();
        let w = parse("大阪", SAMPLE).unwrap();
        c.put("k", 1000, w.clone());
        assert_eq!(c.get("k", 1000 + 3599), Some(w));
        assert!(c.get("k", 1000 + 3600).is_none());
        assert!(c.get("other", 1000).is_none());
    }

    #[tokio::test]
    async fn fetch_from_fake_server() {
        use crate::backends::http::{client, fake_server};
        let city = super::super::city::search_local("大阪").remove(0);
        let (url, rx) = fake_server::serve(200, "application/json", vec![SAMPLE.into()]).await;
        let w = fetch(&client(), &url, &city).await.unwrap();
        assert_eq!(w.city, "大阪");
        // 只发了坐标
        let head = rx.await.unwrap().head;
        assert!(head.contains("latitude=34.6938") && head.contains("longitude=135.5011"));
        assert!(!head.contains("%E5%A4%A7"), "不发城市名");
        let (bad, _rx) = fake_server::serve(500, "text/plain", vec!["x".into()]).await;
        assert!(fetch(&client(), &bad, &city).await.is_none());
    }
}

/// 真实接口冒烟测试（默认忽略；只在天气代码改动后手动跑一次：
/// `cargo test --manifest-path src-tauri/Cargo.toml real_weather_smoke -- --ignored --nocapture`）。
/// Open-Meteo 免费、不需要 Key，不花钱。
#[cfg(test)]
mod smoke {
    #[tokio::test]
    #[ignore]
    async fn real_weather_smoke() {
        let c = super::super::city::search_local("大阪").remove(0);
        let w = super::fetch(&crate::backends::http::client(), super::API, &c)
            .await
            .expect("真实接口没有返回");
        println!("{}", w.for_prompt());
        assert_eq!(w.days.len(), 3);
    }
}
